//! Grouping orchestration over pure membership/layout and shared aggregate kernels.
//! Contracts: <https://support.microsoft.com/en-us/excel/functions/groupby-function>
//! <https://support.microsoft.com/en-us/excel/functions/percentof-function>
use super::selection::compare;
use crate::eval::adapters::elementwise::matrices::failure;
use crate::eval::adapters::elementwise::{
    ArgumentRole, FixedScalar, WholeCollection, collections,
    controls::{EmptyControl, IntegerControl},
    matrices,
};
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::{
    Aggregators, ArgumentCoercion, CallHost, OperandKind, OperandReader, ScalarOperand,
};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::{
    FunctionError, FunctionResult, OperandOrigin, Value, err, numeric_outcome,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Blank, Boolean, Error, Number};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::matrix::{Axis, Budget, FieldRelationship, Matrix};
use poirot_functions::statistics::BuiltinAggregator as A;

fn append<T>(values: &mut Vec<T>, value: T) -> FunctionResult<()> {
    values
        .try_reserve(1)
        .map_err(|reservation_source| EvaluationError::Resource {
            kind: crate::eval::limits::ResourceFailureKind::Allocation,
            capability: CapabilityId::FunctionAdapter,
            target: None,
            reason: (crate::eval::limits::EvaluationReasonError::GroupingOutputAllocationFailed)
                .with_reservation_source(reservation_source),
        })?;
    // SAFETY: The successful local reservation admits exactly this one additional member.
    values.push(value);
    Ok(())
}

fn reserve<T>(host: &mut CallHost<'_, '_>, count: usize) -> FunctionResult<Vec<T>> {
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    Ok(values)
}

fn generated_text(
    host: &mut CallHost<'_, '_>,
    parts: &[crate::model::TextRef<'_>],
) -> FunctionResult<CellValue> {
    let count = parts
        .iter()
        .try_fold(0usize, |total, part| total.checked_add(part.len_units()))
        .ok_or_else(|| host.size_overflow())?;
    if count > 32767 {
        return Err(CellError::Value.into());
    }
    let bytes = count.checked_mul(2).ok_or_else(|| host.size_overflow())?;
    host.check_text(bytes)?;
    host.check_values(count)?;
    host.charge_work(count as u64)?;
    let mut units = Vec::new();
    units
        .try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    for part in parts {
        // SAFETY: count is the checked sum of these unchanged part lengths, and that total capacity was reserved locally.
        units.extend(part.units());
    }
    Ok(CellValue::TextUtf16(crate::model::ExcelText::from_units(
        units,
    )))
}

fn ordinal_label(
    host: &mut CallHost<'_, '_>,
    prefix: &str,
    ordinal: usize,
) -> FunctionResult<CellValue> {
    let number = ArgumentCoercion::new(host).coerce_number_to_text(ordinal as f64)?;
    generated_text(
        host,
        &[
            crate::model::TextRef::Utf8(prefix),
            crate::model::TextRef::Utf8(" "),
            crate::model::TextRef::Utf8(&number),
        ],
    )
}

fn option(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<Option<i64>> {
    FixedScalar::at(index).optional_integer(host, EmptyControl::Omission, CellError::Value)
}

fn normalized(value: CellValue) -> CellValue {
    poirot_functions::coercion::compatibility::coerce_array_member(value)
}

fn aggregate(
    host: &mut CallHost<'_, '_>,
    operation: A,
    members: &[&ScalarOperand],
) -> FunctionResult<CellValue> {
    use poirot_functions::statistics::{Operand, Origin, Value as V};
    host.charge_work(members.len() as u64)?;
    if matches!(operation, A::Count | A::CountA | A::CountBlank) {
        let mut count = 0u64;
        for member in members {
            let value = &member.value;
            let numeric = operation == A::Count
                && (matches!(value, Number(_))
                    || (member.origin == OperandOrigin::Direct
                        && !matches!(value, Blank | Error(_))
                        && match ArgumentCoercion::new(host).coerce_scalar_number(value) {
                            Ok(_) => true,
                            Err(FunctionError::Spreadsheet(_)) => false,
                            Err(error) => return Err(error),
                        }));
            count = count.saturating_add(poirot_functions::statistics::count_member(
                operation,
                numeric,
                *value != Blank,
                value.text_ref().is_some_and(|t| t.len_units() == 0),
            ));
        }
        return Ok(Number(count as f64));
    }
    if operation == A::Concat {
        let mut output = Vec::new();
        for member in members {
            if let Error(error) = member.value {
                return Err(error.into());
            }
            let units = ArgumentCoercion::new(host).coerce_text_units(&member.value)?;
            let length = output
                .len()
                .checked_add(units.len())
                .ok_or_else(|| host.size_overflow())?;
            if length > 32767 {
                return Err(CellError::Value.into());
            }
            host.check_values(length)?;
            host.check_text(length.saturating_mul(2))?;
            output
                .try_reserve(units.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: The successful preceding reservation admits the unchanged units.len() additional members.
            output.extend(units);
        }
        return Ok(CellValue::TextUtf16(crate::model::ExcelText::from_units(
            output,
        )));
    }
    let mut operands = reserve(host, members.len())?;
    for member in members {
        let origin = match member.origin {
            OperandOrigin::Direct => Origin::Direct,
            OperandOrigin::ReferencedCell => Origin::Reference,
            OperandOrigin::ArrayElement => Origin::Array,
        };
        let value = match &member.value {
            Number(n) => V::Number(*n),
            Boolean(b) => V::Boolean(*b),
            Blank => V::Blank,
            Error(error) => V::Error(*error),
            value => V::Text {
                numeric: if origin == Origin::Direct {
                    match ArgumentCoercion::new(host).coerce_scalar_number(value) {
                        Ok(value) => Some(value),
                        Err(FunctionError::Spreadsheet(_)) => None,
                        Err(error) => return Err(error),
                    }
                } else {
                    None
                },
            },
        };
        append(&mut operands, Operand { origin, value })?;
    }
    let collection = operation.collection();
    host.check_values(operands.len())?;
    let mut numeric =
        poirot_functions::statistics::coerce_statistics_collection(collection, &operands)?;
    if matches!(operation, A::Median | A::Mode) {
        let work = poirot_functions::matrix::selection_work(numeric.len(), numeric.len(), 1)
            .ok_or_else(|| host.size_overflow())?;
        host.charge_work(work as u64)?;
    }
    if operation == A::Mode {
        host.check_values(numeric.len().saturating_mul(3))?;
        use poirot_functions::statistics::DistributionError as SampleError;
        poirot_functions::statistics::modern_sample::modes(&mut numeric, host.allocation_failure())
            .map_err(|error| match error {
                SampleError::Spreadsheet(error) => FunctionError::from(error),
                SampleError::Operational(error) => error.into(),
                SampleError::NonConvergence | SampleError::ComputationalLimit => {
                    host.budget_exceeded().into()
                }
            })?;
        return Ok(Number(*numeric.first().ok_or(CellError::NA)?));
    }
    Ok(numeric_outcome::convert_checked_numeric_result(
        operation.numeric(&mut numeric)?,
    ))
}

fn ratio(subset: CellValue, total: CellValue) -> CellValue {
    match (subset, total) {
        (Error(error), _) | (_, Error(error)) => Error(error),
        (Number(_), Number(0.0)) => Error(CellError::Div0),
        (Number(a), Number(b)) => numeric_outcome::convert_checked_numeric_result(a / b),
        _ => Error(CellError::Value),
    }
}

fn aggregate_value(
    host: &mut CallHost<'_, '_>,
    operation: A,
    members: &[&ScalarOperand],
) -> FunctionResult<CellValue> {
    match aggregate(host, operation, members) {
        Err(FunctionError::Spreadsheet(error)) => Ok(Error(error)),
        other => other,
    }
}

fn percentof(host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
    let mut sums = reserve(host, 2)?;
    for index in 0..2 {
        let count = OperandReader::new(host).collection_len(index)?;
        let values = reserve(host, count)?;
        let values = collections::fold(
            host,
            [WholeCollection::bind(index, 0, true)],
            values,
            |_, values, operand| append(values, operand),
        )?;
        let mut members = reserve(host, values.len())?;
        members
            .try_reserve(values.len())
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: The unchanged source iterator emits exactly the locally reserved number of members.
        members.extend(values.iter());
        append(&mut sums, aggregate_value(host, A::Sum, &members)?)?;
    }
    Ok(ratio(
        sums.first().cloned().ok_or(CellError::Value)?,
        sums.get(1).cloned().ok_or(CellError::Value)?,
    )
    .into())
}

/// Separates whether grouping input contains a header from whether output should show one.
/// These choices control record traversal and publication independently.
struct Headers {
    /// Whether row zero contains source labels rather than data.
    consume: bool,
    /// Whether to publish a field-header row.
    show: bool,
}

fn headers(host: &mut CallHost<'_, '_>, fields: usize, rows: usize) -> FunctionResult<Headers> {
    Ok(match option(host, 3)? {
        Some(0) => Headers {
            consume: false,
            show: false,
        },
        Some(1) => Headers {
            consume: true,
            show: false,
        },
        Some(2) => Headers {
            consume: false,
            show: true,
        },
        Some(3) => Headers {
            consume: true,
            show: true,
        },
        None => {
            let first = OperandReader::new(host).read_cell(1, 0, 0)?.value;
            let consume = rows > 1
                && first.text_ref().is_some()
                && matches!(
                    OperandReader::new(host).read_cell(1, 1, 0)?.value,
                    Number(_)
                );
            Headers {
                consume,
                show: fields > 1,
            }
        }
        _ => return Err(CellError::Value.into()),
    })
}

fn groupby(host: &mut CallHost<'_, '_>, address: CellAddress) -> FunctionResult<Value> {
    let key_shape = OperandReader::new(host).describe(0)?;
    let value_shape = OperandReader::new(host).describe(1)?;
    if key_shape.sheets != 1 || value_shape.sheets != 1 || key_shape.rows != value_shape.rows {
        return Err(CellError::Value.into());
    }
    let fields = key_shape.columns;
    let value_columns = value_shape.columns;
    let aggregators = Aggregators::resolve(host, 2)?;
    let headers = headers(host, fields, key_shape.rows)?;
    let relationship = match option(host, 7)?.unwrap_or(0) {
        0 => FieldRelationship::Hierarchy,
        1 => FieldRelationship::Table,
        _ => return Err(CellError::Value.into()),
    };
    let total_depth = option(host, 4)?.unwrap_or(if relationship == FieldRelationship::Hierarchy {
        fields as i64
    } else {
        1
    });
    if total_depth.unsigned_abs() > fields as u64
        || (relationship == FieldRelationship::Table && total_depth.unsigned_abs() > 1)
    {
        return Err(CellError::Value.into());
    }
    let included =
        WholeCollection::at(6).selected_rows(host, key_shape.rows, usize::from(headers.consume))?;
    if included.is_empty() {
        return Err(CellError::Calc.into());
    }
    let key_count = included
        .len()
        .checked_mul(fields)
        .ok_or_else(|| host.size_overflow())?;
    let keys = matrices::collect(host, 0, included.iter().copied(), fields, |_, operand| {
        Ok(operand.value)
    })?;
    let values = matrices::collect(
        host,
        1,
        included.iter().copied(),
        value_columns,
        |_, operand| Ok(operand),
    )?;
    let work = poirot_functions::matrix::selection_work(key_count, included.len(), fields)
        .ok_or_else(|| host.size_overflow())?;
    host.charge_work(work as u64)?;
    let partition = poirot_functions::matrix::partition(
        &keys,
        Axis::Rows,
        compare,
        Budget {
            max_cells: key_count,
            max_work: work,
        },
    )
    .map_err(|e| failure(host, e))?;
    let hierarchy_count = key_count
        .checked_add(1)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(hierarchy_count)?;
    host.charge_work(hierarchy_count as u64)?;
    let groups = poirot_functions::matrix::hierarchy(
        &keys,
        &partition,
        compare,
        Budget {
            max_cells: hierarchy_count,
            max_work: hierarchy_count,
        },
    )
    .map_err(|e| failure(host, e))?;
    let sortable_columns = fields
        .checked_add(value_columns)
        .ok_or_else(|| host.size_overflow())?;
    let reductions = value_columns
        .checked_mul(aggregators.members.len())
        .ok_or_else(|| host.size_overflow())?;
    let scratch = groups
        .len()
        .checked_mul(reductions)
        .ok_or_else(|| host.size_overflow())?;
    let mut results = reserve(host, scratch)?;
    let mut denominators = reserve(host, value_columns)?;
    if aggregators.members.contains(&A::PercentOf) {
        for column in 0..value_columns {
            let mut members = reserve(host, included.len())?;
            for row in 0..included.len() {
                append(
                    &mut members,
                    values.get(row, column).map_err(|e| failure(host, e))?,
                )?;
            }
            append(&mut denominators, aggregate_value(host, A::Sum, &members)?)?;
        }
    }
    for group in &groups {
        let mut ordinals = reserve(host, group.members.len())?;
        ordinals
            .try_reserve(group.members.len())
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: The source range below has exactly the locally reserved member count.
        ordinals.extend_from_slice(
            partition
                .order
                .get(group.members.clone())
                .ok_or(CellError::Value)?,
        );
        let work = poirot_functions::matrix::selection_work(ordinals.len(), ordinals.len(), 1)
            .ok_or_else(|| host.size_overflow())?;
        host.charge_work(work as u64)?;
        // SAFETY: Source ordinals use a fixed usize total order, independent of worksheet comparisons.
        ordinals.sort_unstable();
        for column in 0..value_columns {
            let mut members = reserve(host, ordinals.len())?;
            for &row in &ordinals {
                append(
                    &mut members,
                    values.get(row, column).map_err(|e| failure(host, e))?,
                )?;
            }
            for &operation in &aggregators.members {
                let value = aggregate_value(host, operation, &members)?;
                append(
                    &mut results,
                    if operation == A::PercentOf {
                        ratio(
                            value,
                            denominators.get(column).cloned().ok_or(CellError::Value)?,
                        )
                    } else {
                        value
                    },
                )?;
            }
        }
    }
    let results = Matrix::new(groups.len(), reductions, results).map_err(|e| failure(host, e))?;
    let mut sorts = reserve(host, fields)?;
    if host.len() > 5 && host.kind(5)? != OperandKind::Missing {
        let shape = OperandReader::new(host).describe(5)?;
        if shape.sheets != 1 || (shape.rows != 1 && shape.columns != 1) {
            return Err(CellError::Value.into());
        }
        let selectors =
            WholeCollection::at(5).integers(host, IntegerControl::Exact, |n, count| {
                if n == 0
                    || n.unsigned_abs() > sortable_columns as u64
                    || count > 1 && n.unsigned_abs() > fields as u64
                {
                    return Err(CellError::Value.into());
                }
                Ok(())
            })?;
        for selector in selectors.into_values() {
            append(&mut sorts, selector)?;
        }
    } else {
        for field in 1..=fields {
            append(&mut sorts, field as i64)?;
        }
    }
    let representative = |group: usize| {
        groups
            .get(group)
            .and_then(|g| partition.order.get(g.members.start))
            .copied()
    };
    let comparison = |left: usize, right: usize| {
        let pair = representative(left).zip(representative(right));
        for &sort in &sorts {
            // SAFETY: Every explicit sort was checked nonzero above; defaults are 1..=fields.
            let index = sort.unsigned_abs() as usize - 1;
            if relationship == FieldRelationship::Hierarchy
                && index < fields
                && groups.get(left).is_some_and(|group| index >= group.depth)
            {
                continue;
            }
            let pair = if index < fields {
                pair.and_then(|(a, b)| Some((keys.get(a, index).ok()?, keys.get(b, index).ok()?)))
            } else {
                results
                    .get(
                        left,
                        index
                            .checked_sub(fields)
                            .and_then(|n| n.checked_mul(aggregators.members.len()))
                            .unwrap_or(usize::MAX),
                    )
                    .ok()
                    .zip(
                        results
                            .get(
                                right,
                                index
                                    .checked_sub(fields)
                                    .and_then(|n| n.checked_mul(aggregators.members.len()))
                                    .unwrap_or(usize::MAX),
                            )
                            .ok(),
                    )
            };
            if let Some((a, b)) = pair {
                let order = compare(a, b);
                let order = if sort < 0 { order.reverse() } else { order };
                if order != std::cmp::Ordering::Equal {
                    return order;
                }
            }
        }
        std::cmp::Ordering::Equal
    };
    let work = poirot_functions::matrix::selection_work(groups.len(), groups.len(), 1)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(groups.len().saturating_mul(2))?;
    host.charge_work(work.saturating_mul(sorts.len().max(1)) as u64)?;
    let layout = poirot_functions::matrix::group_layout(
        &groups,
        fields,
        relationship,
        total_depth,
        comparison,
        Budget {
            max_cells: groups.len().saturating_mul(2),
            max_work: work,
        },
    )
    .map_err(|e| failure(host, e))?;
    Output {
        address,
        keys: &keys,
        partition: &partition,
        groups: &groups,
        results: &results,
        layout: &layout,
        aggregators: &aggregators,
        value_columns,
        headers,
    }
    .publish(host)
}

/// Combines grouping keys, hierarchy, reductions, and layout for final worksheet publication.
/// It borrows the intermediate results so publication can validate dimensions and add headers without rebuilding the groups.
struct Output<'a> {
    address: CellAddress,
    keys: &'a Matrix<CellValue>,
    partition: &'a poirot_functions::matrix::Partition,
    groups: &'a [poirot_functions::matrix::Group],
    results: &'a Matrix<CellValue>,
    layout: &'a [usize],
    aggregators: &'a Aggregators,
    value_columns: usize,
    headers: Headers,
}

impl Output<'_> {
    fn publish(self, host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
        let Self {
            address,
            keys,
            partition,
            groups,
            results,
            layout,
            aggregators,
            value_columns,
            headers,
        } = self;
        if value_columns.checked_mul(aggregators.members.len()) != Some(results.cols())
            || aggregators.rows.checked_mul(aggregators.columns) != Some(aggregators.members.len())
            || aggregators.members.is_empty()
            || results.rows() != groups.len()
        {
            return Err(EvaluationError::StalePlan.into());
        }
        let vertical = aggregators.columns == 1 && aggregators.rows > 1;
        let fields = keys.cols();
        let extra_label = usize::from(vertical);
        let columns = fields
            .checked_add(extra_label)
            .and_then(|n| {
                n.checked_add(if vertical {
                    value_columns
                } else {
                    results.cols()
                })
            })
            .ok_or_else(|| host.size_overflow())?;
        let rows = layout
            .len()
            .checked_mul(aggregators.rows)
            .and_then(|n| n.checked_add(usize::from(headers.show)))
            .ok_or_else(|| host.size_overflow())?;
        if rows > crate::model::MAX_ROWS as usize || columns > crate::model::MAX_COLUMNS as usize {
            return Err(CellError::Num.into());
        }
        let count = rows
            .checked_mul(columns)
            .ok_or_else(|| host.size_overflow())?;
        let mut output = reserve(host, count)?;
        if headers.show {
            for field in 0..fields {
                append(
                    &mut output,
                    if headers.consume {
                        normalized(OperandReader::new(host).read_cell(0, 0, field)?.value)
                    } else {
                        ordinal_label(host, "Field", field.saturating_add(1))?
                    },
                )?;
            }
            if vertical {
                append(
                    &mut output,
                    generated_text(host, &[crate::model::TextRef::Utf8("Function")])?,
                )?;
            }
            for column in 0..value_columns {
                for &operation in aggregators.members.iter().take(if vertical {
                    1
                } else {
                    aggregators.members.len()
                }) {
                    let value = if headers.consume {
                        normalized(OperandReader::new(host).read_cell(1, 0, column)?.value)
                    } else {
                        ordinal_label(host, "Value", column.saturating_add(1))?
                    };
                    if aggregators.members.len() > 1 && !vertical {
                        let label = ArgumentCoercion::new(host).coerce_text_units(&value)?;
                        append(
                            &mut output,
                            generated_text(
                                host,
                                &[
                                    crate::model::TextRef::Utf16(&label),
                                    crate::model::TextRef::Utf8(" "),
                                    crate::model::TextRef::Utf8(operation.name()),
                                ],
                            )?,
                        )?;
                    } else {
                        append(&mut output, value)?;
                    }
                }
            }
        }
        for &index in layout {
            let group = groups.get(index).ok_or(CellError::Value)?;
            let representative = *partition
                .order
                .get(group.members.start)
                .ok_or(CellError::Value)?;
            for function in 0..aggregators.rows {
                for field in 0..fields {
                    append(
                        &mut output,
                        if field < group.depth {
                            let value = keys
                                .get(representative, field)
                                .map_err(|e| failure(host, e))?;
                            normalized(host.copy_cell_value(value)?)
                        } else if field == group.depth {
                            generated_text(
                                host,
                                &[crate::model::TextRef::Utf8(if group.depth == 0 {
                                    "Total"
                                } else {
                                    "Subtotal"
                                })],
                            )?
                        } else {
                            Blank
                        },
                    )?;
                }
                if vertical {
                    let name = aggregators
                        .members
                        .get(function)
                        .ok_or(CellError::Value)?
                        .name();
                    append(
                        &mut output,
                        generated_text(host, &[crate::model::TextRef::Utf8(name)])?,
                    )?;
                }
                for column in 0..value_columns {
                    for function in if vertical {
                        function..function.saturating_add(1)
                    } else {
                        0..aggregators.members.len()
                    } {
                        // SAFETY: The local checked product value_columns * member_count equals results.cols();
                        // both loop indices are strictly below their corresponding factors.
                        let result = results
                            .get(index, column * aggregators.members.len() + function)
                            .map_err(|e| failure(host, e))?;
                        append(&mut output, host.copy_cell_value(result)?)?;
                    }
                }
            }
        }
        if output.len() != count {
            return Err(EvaluationError::StalePlan.into());
        }
        matrices::array(host, rows, columns, output.into_iter(), address)
    }
}

/// Execute grouping or the standalone sum ratio under normal spill and failure policies.
pub(in crate::eval) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if (function == MatrixFunction::Groupby && !(3..=8).contains(&args.len()))
        || (function == MatrixFunction::Percentof && args.len() != 2)
    {
        return Ok(err(CellError::Value));
    }
    let mut host = CallHost::new_array_slots(session, sheet, address, args, &[0, 1, 5, 6]);
    match if function == MatrixFunction::Percentof {
        percentof(&mut host)
    } else {
        groupby(&mut host, address)
    } {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}
