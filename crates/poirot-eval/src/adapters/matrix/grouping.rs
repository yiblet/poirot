//! Grouping orchestration over pure membership/layout and shared aggregate kernels.
//! Contracts: https://support.microsoft.com/en-us/excel/functions/groupby-function
//! https://support.microsoft.com/en-us/excel/functions/percentof-function
use super::modern::failure;
use super::selection::compare;
use super::*;
use crate::host::{Aggregators, MissingPolicy, OperandOrigin, ScalarOperand};
use crate::registry::MatrixFunction as F;
use poirot_functions_matrix::{Axis, Budget, FieldRelationship, Matrix};
use poirot_functions_statistics::BuiltinAggregator as A;

fn append<T>(values: &mut Vec<T>, value: T) -> FunctionResult<()> {
    values
        .try_reserve(1)
        .map_err(|_| EvaluationFailure::ResourceLimit {
            capability: CapabilityId::FunctionAdapter,
            source: None,
            detail: "grouping output allocation failed".into(),
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
        .map_err(|_| host.budget_exceeded())?;
    Ok(values)
}
fn generated_text(
    host: &mut CallHost<'_, '_>,
    parts: &[poirot_model::TextRef<'_>],
) -> FunctionResult<CellValue> {
    let count = parts
        .iter()
        .try_fold(0usize, |total, part| total.checked_add(part.len_units()))
        .ok_or_else(|| host.budget_exceeded())?;
    if count > 32767 {
        return Err(CellError::Value.into());
    }
    let bytes = count.checked_mul(2).ok_or_else(|| host.budget_exceeded())?;
    host.check_text(bytes)?;
    host.check_values(count)?;
    host.charge_work(count as u64)?;
    let mut units = Vec::new();
    units
        .try_reserve_exact(count)
        .map_err(|_| host.budget_exceeded())?;
    for part in parts {
        // SAFETY: count is the checked sum of these unchanged part lengths, and that total capacity was reserved locally.
        units.extend(part.units());
    }
    Ok(CellValue::TextUtf16(poirot_model::ExcelText::from_units(
        units,
    )))
}
fn ordinal_label(
    host: &mut CallHost<'_, '_>,
    prefix: &str,
    ordinal: usize,
) -> FunctionResult<CellValue> {
    let number = host.coercev2_number_to_text(ordinal as f64, NumericPolicy::PinnedPoi)?;
    generated_text(
        host,
        &[
            poirot_model::TextRef::Utf8(prefix),
            poirot_model::TextRef::Utf8(" "),
            poirot_model::TextRef::Utf8(&number),
        ],
    )
}
fn option(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<Option<i64>> {
    if index >= host.len() {
        return Ok(None);
    }
    let operand = host.scalar_data(index, MissingPolicy::Blank)?;
    if operand.missing {
        return Ok(None);
    }
    let number = host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)?;
    if !number.is_finite() || number.abs() >= i64::MAX as f64 {
        return Err(CellError::Value.into());
    }
    Ok(Some(number.trunc() as i64))
}
fn normalized(value: CellValue) -> CellValue {
    poirot_coercionv2::compatibility::coercev2_array_member(value)
}
fn aggregate(
    host: &mut CallHost<'_, '_>,
    operation: A,
    members: &[&ScalarOperand],
) -> FunctionResult<CellValue> {
    use poirot_functions_statistics::{Operand, Origin, Value as V};
    host.charge_work(members.len() as u64)?;
    if matches!(operation, A::Count | A::CountA | A::CountBlank) {
        let mut count = 0u64;
        for member in members {
            let value = &member.value;
            let numeric = operation == A::Count
                && (matches!(value, Number(_))
                    || (member.origin == OperandOrigin::Direct
                        && !matches!(value, Blank | Error(_))
                        && match host.coercev2_scalar_number(value, NumericPolicy::PinnedPoi) {
                            Ok(_) => true,
                            Err(FunctionFailure::Spreadsheet(_)) => false,
                            Err(error) => return Err(error),
                        }));
            count = count.saturating_add(poirot_functions_statistics::count_member(
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
            let units = host.coercev2_text_units(&member.value)?;
            let length = output
                .len()
                .checked_add(units.len())
                .ok_or_else(|| host.budget_exceeded())?;
            if length > 32767 {
                return Err(CellError::Value.into());
            }
            host.check_values(length)?;
            host.check_text(length.saturating_mul(2))?;
            output
                .try_reserve(units.len())
                .map_err(|_| host.budget_exceeded())?;
            // SAFETY: The successful preceding reservation admits the unchanged units.len() additional members.
            output.extend(units);
        }
        return Ok(CellValue::TextUtf16(poirot_model::ExcelText::from_units(
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
                    match host.coercev2_scalar_number(value, NumericPolicy::PinnedPoi) {
                        Ok(value) => Some(value),
                        Err(FunctionFailure::Spreadsheet(_)) => None,
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
        poirot_functions_statistics::coercev2_statistics_collection(collection, &operands)?;
    if matches!(operation, A::Median | A::Mode) {
        let work = poirot_functions_matrix::selection_work(numeric.len(), numeric.len(), 1)
            .ok_or_else(|| host.budget_exceeded())?;
        host.charge_work(work as u64)?;
    }
    Ok(numeric_outcome::coerce_checked_numeric_result(
        operation.numeric(&mut numeric)?,
    ))
}
fn ratio(subset: CellValue, total: CellValue) -> CellValue {
    match (subset, total) {
        (Error(error), _) | (_, Error(error)) => Error(error),
        (Number(_), Number(0.0)) => Error(CellError::Div0),
        (Number(a), Number(b)) => numeric_outcome::coerce_checked_numeric_result(a / b),
        _ => Error(CellError::Value),
    }
}
fn aggregate_value(
    host: &mut CallHost<'_, '_>,
    operation: A,
    members: &[&ScalarOperand],
) -> FunctionResult<CellValue> {
    match aggregate(host, operation, members) {
        Err(FunctionFailure::Spreadsheet(error)) => Ok(Error(error)),
        other => other,
    }
}
fn percentof(host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
    let mut sums = reserve(host, 2)?;
    for index in 0..2 {
        let count = host.collection_len(index)?;
        let mut values = reserve(host, count)?;
        for offset in 0..count {
            append(&mut values, host.collection_cell(index, offset)?)?;
        }
        let mut members = reserve(host, values.len())?;
        members
            .try_reserve(values.len())
            .map_err(|_| host.budget_exceeded())?;
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
/// Header consumption and publication are distinct from source geometry.
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
            let first = host.read_cell(1, 0, 0)?.value;
            let consume = rows > 1
                && first.text_ref().is_some()
                && matches!(host.read_cell(1, 1, 0)?.value, Number(_));
            Headers {
                consume,
                show: fields > 1,
            }
        }
        _ => return Err(CellError::Value.into()),
    })
}
fn groupby(host: &mut CallHost<'_, '_>, address: CellAddress) -> FunctionResult<Value> {
    let key_shape = host.describe(0)?;
    let value_shape = host.describe(1)?;
    if key_shape.sheets != 1 || value_shape.sheets != 1 || key_shape.rows != value_shape.rows {
        return Err(CellError::Value.into());
    }
    let fields = key_shape.columns;
    let value_columns = value_shape.columns;
    let aggregators = host.aggregators(2)?;
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
    let mut included = reserve(host, key_shape.rows)?;
    let filter = if host.len() > 6 {
        host.kind(6)? != OperandKind::Missing
    } else {
        false
    };
    if filter {
        let shape = host.describe(6)?;
        if shape.sheets != 1 || shape.columns != 1 || shape.rows != key_shape.rows {
            return Err(CellError::Value.into());
        }
    }
    for row in usize::from(headers.consume)..key_shape.rows {
        if !filter || core_projection::coercev2_operand_boolean(&host.read_cell(6, row, 0)?.value)?
        {
            append(&mut included, row)?;
        }
    }
    if included.is_empty() {
        return Err(CellError::Calc.into());
    }
    let key_count = included
        .len()
        .checked_mul(fields)
        .ok_or_else(|| host.budget_exceeded())?;
    let value_count = included
        .len()
        .checked_mul(value_columns)
        .ok_or_else(|| host.budget_exceeded())?;
    let mut key_values = reserve(host, key_count)?;
    let mut values = reserve(host, value_count)?;
    for &row in &included {
        for column in 0..fields {
            append(&mut key_values, host.read_cell(0, row, column)?.value)?;
        }
        for column in 0..value_columns {
            append(&mut values, host.read_cell(1, row, column)?)?;
        }
    }
    let keys = Matrix::new(included.len(), fields, key_values).map_err(|e| failure(host, e))?;
    let values =
        Matrix::new(included.len(), value_columns, values).map_err(|e| failure(host, e))?;
    let work = poirot_functions_matrix::selection_work(key_count, included.len(), fields)
        .ok_or_else(|| host.budget_exceeded())?;
    host.charge_work(work as u64)?;
    let partition = poirot_functions_matrix::partition(
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
        .ok_or_else(|| host.budget_exceeded())?;
    host.check_values(hierarchy_count)?;
    host.charge_work(hierarchy_count as u64)?;
    let groups = poirot_functions_matrix::hierarchy(
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
        .ok_or_else(|| host.budget_exceeded())?;
    let reductions = value_columns
        .checked_mul(aggregators.members.len())
        .ok_or_else(|| host.budget_exceeded())?;
    let scratch = groups
        .len()
        .checked_mul(reductions)
        .ok_or_else(|| host.budget_exceeded())?;
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
            .map_err(|_| host.budget_exceeded())?;
        // SAFETY: The source range below has exactly the locally reserved member count.
        ordinals.extend_from_slice(
            partition
                .order
                .get(group.members.clone())
                .ok_or(CellError::Value)?,
        );
        let work = poirot_functions_matrix::selection_work(ordinals.len(), ordinals.len(), 1)
            .ok_or_else(|| host.budget_exceeded())?;
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
        let shape = host.describe(5)?;
        if shape.sheets != 1 || (shape.rows != 1 && shape.columns != 1) {
            return Err(CellError::Value.into());
        }
        let count = shape
            .rows
            .checked_mul(shape.columns)
            .ok_or_else(|| host.budget_exceeded())?;
        host.check_values(count)?;
        for row in 0..shape.rows {
            for column in 0..shape.columns {
                let member = host.read_cell(5, row, column)?;
                let n = host.coercev2_scalar_number(&member.value, NumericPolicy::PinnedPoi)?;
                if !n.is_finite()
                    || n == 0.0
                    || n.fract() != 0.0
                    || n.abs() > sortable_columns as f64
                    || count > 1 && n.abs() > fields as f64
                {
                    return Err(CellError::Value.into());
                }
                sorts.try_reserve(1).map_err(|_| host.budget_exceeded())?;
                append(&mut sorts, n as i64)?;
            }
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
    let work = poirot_functions_matrix::selection_work(groups.len(), groups.len(), 1)
        .ok_or_else(|| host.budget_exceeded())?;
    host.check_values(groups.len().saturating_mul(2))?;
    host.charge_work(work.saturating_mul(sorts.len().max(1)) as u64)?;
    let layout = poirot_functions_matrix::group_layout(
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
/// Borrowed grouping layout and reductions ready for bounded worksheet publication.
struct Output<'a> {
    address: CellAddress,
    keys: &'a Matrix<CellValue>,
    partition: &'a poirot_functions_matrix::Partition,
    groups: &'a [poirot_functions_matrix::Group],
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
            return Err(EvaluationFailure::StalePlan.into());
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
            .ok_or_else(|| host.budget_exceeded())?;
        let rows = layout
            .len()
            .checked_mul(aggregators.rows)
            .and_then(|n| n.checked_add(usize::from(headers.show)))
            .ok_or_else(|| host.budget_exceeded())?;
        if rows > poirot_model::MAX_ROWS as usize || columns > poirot_model::MAX_COLUMNS as usize {
            return Err(CellError::Num.into());
        }
        let count = rows
            .checked_mul(columns)
            .ok_or_else(|| host.budget_exceeded())?;
        let mut output = reserve(host, count)?;
        if headers.show {
            for field in 0..fields {
                append(
                    &mut output,
                    if headers.consume {
                        normalized(host.read_cell(0, 0, field)?.value)
                    } else {
                        ordinal_label(host, "Field", field.saturating_add(1))?
                    },
                )?;
            }
            if vertical {
                append(
                    &mut output,
                    generated_text(host, &[poirot_model::TextRef::Utf8("Function")])?,
                )?;
            }
            for column in 0..value_columns {
                for &operation in aggregators.members.iter().take(if vertical {
                    1
                } else {
                    aggregators.members.len()
                }) {
                    let value = if headers.consume {
                        normalized(host.read_cell(1, 0, column)?.value)
                    } else {
                        ordinal_label(host, "Value", column.saturating_add(1))?
                    };
                    if aggregators.members.len() > 1 && !vertical {
                        let label = host.coercev2_text_units(&value)?;
                        append(
                            &mut output,
                            generated_text(
                                host,
                                &[
                                    poirot_model::TextRef::Utf16(&label),
                                    poirot_model::TextRef::Utf8(" "),
                                    poirot_model::TextRef::Utf8(operation.name()),
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
                                &[poirot_model::TextRef::Utf8(if group.depth == 0 {
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
                        generated_text(host, &[poirot_model::TextRef::Utf8(name)])?,
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
            return Err(EvaluationFailure::StalePlan.into());
        }
        let mut cells = output.into_iter();
        let mut published = reserve(host, rows)?;
        for _ in 0..rows {
            let mut row = reserve(host, columns)?;
            for _ in 0..columns {
                append(&mut row, cells.next().ok_or(EvaluationFailure::StalePlan)?)?;
            }
            append(&mut published, row)?;
        }
        Ok(Value::Array(ArrayView::anchored(published, Some(address))))
    }
}
/// Execute grouping or the standalone sum ratio under normal spill and failure policies.
pub(crate) fn execute(
    function: F,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if (function == F::Groupby && !(3..=8).contains(&args.len()))
        || (function == F::Percentof && args.len() != 2)
    {
        return Ok(err(CellError::Value));
    }
    let mut host = CallHost::new_array_slots(session, sheet, address, args, &[0, 1, 5, 6]);
    match if function == F::Percentof {
        percentof(&mut host)
    } else {
        groupby(&mut host, address)
    } {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
