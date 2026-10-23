//! Modern text adapters preserve deferred arguments, UTF-16 and spill geometry.
//! Extraction delimiter arrays currently broadcast. Whether native Excel treats
//! them as delimiter sets is an open contract question tracked by audit 024.
use crate::host::{CallHost, MissingPolicy, NumericPolicy, OperandDescription, ScalarOperand};
use crate::*;
use poirot_functions_text::modern::{self, Case, Side, Split};
use poirot_model::ExcelText;

fn failure(host: &CallHost<'_, '_>, error: modern::Error) -> FunctionFailure {
    match error {
        modern::Error::Value => CellError::Value.into(),
        modern::Error::NotFound => CellError::NA.into(),
        modern::Error::Empty => CellError::Calc.into(),
        modern::Error::SizeOverflow => host.size_overflow().into(),
        modern::Error::AllocationFailure => host.allocation_failure().into(),
        modern::Error::InvalidGeometry => EvaluationFailure::StalePlan.into(),
    }
}
fn admit_text(host: &mut CallHost<'_, '_>, units: usize) -> FunctionResult<()> {
    let bytes = units.checked_mul(2).ok_or_else(|| host.size_overflow())?;
    host.check_text(bytes)?;
    host.check_values(units)
}
fn coercev2_text_units(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    errors: bool,
) -> FunctionResult<Vec<u16>> {
    if let Some(text) = value.text_ref() {
        admit_text(host, text.len_units())?;
        return Ok(text.units().collect());
    }
    let text = match value {
        Error(error) if errors => error.to_string(),
        Error(error) => return Err((*error).into()),
        Number(number) => host.coercev2_number_to_text(*number, NumericPolicy::PinnedPoi)?,
        _ => host.coercev2_scalar_text(value)?,
    };
    admit_text(host, text.encode_utf16().count())?;
    Ok(text.encode_utf16().collect())
}
fn text_value(units: Vec<u16>) -> CellValue {
    CellValue::TextUtf16(ExcelText::from_units(units))
}
pub(super) fn description(
    host: &mut CallHost<'_, '_>,
    index: usize,
) -> FunctionResult<OperandDescription> {
    let shape = host.describe(index)?;
    if shape.sheets != 1 || shape.rows == 0 || shape.columns == 0 {
        return Err(CellError::Value.into());
    }
    Ok(shape)
}
pub(super) fn member(
    host: &mut CallHost<'_, '_>,
    index: usize,
    shape: OperandDescription,
    row: usize,
    column: usize,
) -> FunctionResult<ScalarOperand> {
    if shape.rows != 1 && row >= shape.rows || shape.columns != 1 && column >= shape.columns {
        return Err(CellError::NA.into());
    }
    host.read_cell(
        index,
        if shape.rows == 1 { 0 } else { row },
        if shape.columns == 1 { 0 } else { column },
    )
}
pub(super) fn number(
    host: &mut CallHost<'_, '_>,
    operand: ScalarOperand,
    default: f64,
) -> FunctionResult<f64> {
    if operand.missing {
        Ok(default)
    } else {
        host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)
    }
}
fn mode(n: f64) -> FunctionResult<Case> {
    match n {
        0. => Ok(Case::Sensitive),
        1. => Ok(Case::Insensitive),
        _ => Err(CellError::Value.into()),
    }
}
fn bool_mode(n: f64) -> FunctionResult<bool> {
    match n {
        0. => Ok(false),
        1. => Ok(true),
        _ => Err(CellError::Value.into()),
    }
}
fn coercev2_option_number(
    host: &mut CallHost<'_, '_>,
    index: usize,
    default: f64,
) -> FunctionResult<f64> {
    let operand = host.scalar_data(index, MissingPolicy::Blank)?;
    number(host, operand, default)
}
pub(super) fn shape_admission(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    columns: usize,
) -> FunctionResult<()> {
    if rows > poirot_model::MAX_ROWS as usize || columns > poirot_model::MAX_COLUMNS as usize {
        return Err(CellError::Num.into());
    }
    let count = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    host.check_values(rows)
}
pub(super) fn output(
    host: &mut CallHost<'_, '_>,
    values: Vec<CellValue>,
    rows: usize,
    columns: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    if rows == 1 && columns == 1 {
        return values
            .into_iter()
            .next()
            .map(Value::Scalar)
            .ok_or(CellError::Value.into());
    }
    shape_admission(host, rows, columns)?;
    if columns == 0 {
        return Err(CellError::Value.into());
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(rows)
        .map_err(|_| host.allocation_failure())?;
    let mut values = values.into_iter();
    for _ in 0..rows {
        let mut row = Vec::new();
        row.try_reserve_exact(columns)
            .map_err(|_| host.allocation_failure())?;
        for _ in 0..columns {
            // SAFETY: The local columns reservation bounds exactly columns pushes.
            row.push(values.next().ok_or(CellError::Value)?);
        }
        // SAFETY: The local rows reservation bounds exactly rows pushes.
        result.push(row);
    }
    Ok(Value::Array(ArrayView::anchored(result, Some(address))))
}
/// A successful element or a demand to evaluate the optional fallback.
enum Extraction {
    Value(CellValue),
    Fallback,
}
fn extract_member(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    shapes: &[OperandDescription],
    row: usize,
    column: usize,
) -> FunctionResult<Extraction> {
    let input = member(
        host,
        0,
        *shapes.first().ok_or(CellError::Value)?,
        row,
        column,
    )?;
    let delimiter = member(
        host,
        1,
        *shapes.get(1).ok_or(CellError::Value)?,
        row,
        column,
    )?;
    let text = coercev2_text_units(host, &input.value, false)?;
    let delimiter = coercev2_text_units(host, &delimiter.value, false)?;
    let mut options = [1., 0., 0.];
    for (ordinal, shape) in shapes.iter().enumerate().skip(2) {
        let operand = member(host, ordinal, *shape, row, column)?;
        let default = if ordinal == 2 { 1. } else { 0. };
        *options
            .get_mut(ordinal.checked_sub(2).ok_or(CellError::Value)?)
            .ok_or(CellError::Value)? = number(host, operand, default)?;
    }
    let occurrence = *options.first().ok_or(CellError::Value)?;
    if !occurrence.is_finite() || occurrence.abs() >= i64::MAX as f64 {
        return Err(CellError::Value.into());
    }
    let case = mode(*options.get(1).ok_or(CellError::Value)?)?;
    let match_end = bool_mode(*options.get(2).ok_or(CellError::Value)?)?;
    let work = text
        .len()
        .checked_add(1)
        .and_then(|n| n.checked_mul(delimiter.len().max(1)))
        .ok_or_else(|| host.size_overflow())?;
    host.charge_work(work as u64)?;
    match modern::extract(
        &text,
        &delimiter,
        occurrence.trunc() as i64,
        case,
        match_end,
        if name == TextFunction::TextBefore {
            Side::Before
        } else {
            Side::After
        },
    ) {
        Ok(span) => {
            let selected = text.get(span).ok_or(CellError::Value)?;
            admit_text(host, selected.len())?;
            Ok(Extraction::Value(text_value(selected.to_vec())))
        }
        Err(modern::Error::NotFound) => Ok(Extraction::Fallback),
        Err(error) => Err(failure(host, error)),
    }
}
fn extraction(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    host.check_values(length.min(5))?;
    let mut shapes = Vec::new();
    shapes
        .try_reserve_exact(length.min(5))
        .map_err(|_| host.allocation_failure())?;
    let mut rows = 1;
    let mut columns = 1;
    for ordinal in 0..length.min(5) {
        let shape = description(host, ordinal)?;
        rows = rows.max(shape.rows);
        columns = columns.max(shape.columns);
        // SAFETY: The loop has exactly length.min(5) iterations, reserved locally.
        shapes.push(shape);
    }
    shape_admission(host, rows, columns)?;
    let mut extracted = Vec::new();
    extracted
        .try_reserve_exact(
            rows.checked_mul(columns)
                .ok_or_else(|| host.size_overflow())?,
        )
        .map_err(|_| host.allocation_failure())?;
    let mut needs_fallback = false;
    for row in 0..rows {
        for column in 0..columns {
            let value = match extract_member(host, name, &shapes, row, column) {
                Ok(Extraction::Fallback) => {
                    needs_fallback = true;
                    Extraction::Fallback
                }
                Ok(value) => value,
                Err(FunctionFailure::Spreadsheet(error)) => Extraction::Value(Error(error)),
                Err(error) => return Err(error),
            };
            // SAFETY: The local checked rows*columns reservation bounds the nested loop's pushes.
            extracted.push(value);
        }
    }
    let fallback_shape = if needs_fallback && length > 5 {
        Some(description(host, 5)?)
    } else {
        None
    };
    let output_rows = fallback_shape.map_or(rows, |s| s.rows.max(rows));
    let output_columns = fallback_shape.map_or(columns, |s| s.columns.max(columns));
    shape_admission(host, output_rows, output_columns)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(
            output_rows
                .checked_mul(output_columns)
                .ok_or_else(|| host.size_overflow())?,
        )
        .map_err(|_| host.allocation_failure())?;
    for row in 0..output_rows {
        for column in 0..output_columns {
            let source_row = if rows == 1 { 0 } else { row };
            let source_column = if columns == 1 { 0 } else { column };
            let selected = if source_row < rows && source_column < columns {
                source_row
                    .checked_mul(columns)
                    .and_then(|n| n.checked_add(source_column))
                    .and_then(|n| extracted.get(n))
            } else {
                None
            };
            let value = match selected {
                Some(Extraction::Value(value)) => {
                    host.check_text(value.text_ref().map_or(0, |t| t.storage_bytes()))?;
                    value.clone()
                }
                Some(Extraction::Fallback) => match fallback_shape {
                    Some(shape) => match member(host, 5, shape, row, column) {
                        Ok(operand) => operand.value,
                        Err(FunctionFailure::Spreadsheet(error)) => Error(error),
                        Err(error) => return Err(error),
                    },
                    None => Error(CellError::NA),
                },
                None => Error(CellError::NA),
            };
            values.push(poirot_coercionv2::compatibility::coercev2_array_member(
                value,
            ));
        }
    }
    output(host, values, output_rows, output_columns, address)
}
fn delimiters(host: &mut CallHost<'_, '_>, ordinal: usize) -> FunctionResult<Vec<Vec<u16>>> {
    let shape = description(host, ordinal)?;
    if shape.missing {
        return Ok(Vec::new());
    }
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for row in 0..shape.rows {
        for column in 0..shape.columns {
            let operand = host.read_cell(ordinal, row, column)?;
            // SAFETY: count is the local checked shape product, reserved before this traversal.
            values.push(coercev2_text_units(host, &operand.value, false)?);
        }
    }
    Ok(values)
}
fn split(
    host: &mut CallHost<'_, '_>,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    let source = description(host, 0)?;
    // A variable number of output members cannot form nested arrays of split results.
    if source.rows != 1 || source.columns != 1 {
        return Err(CellError::Value.into());
    }
    let input = host.read_cell(0, 0, 0)?;
    let text = coercev2_text_units(host, &input.value, false)?;
    let columns = delimiters(host, 1)?;
    let rows = if length > 2 {
        delimiters(host, 2)?
    } else {
        Vec::new()
    };
    let ignore_empty = if length > 3 {
        let operand = host.scalar_data(3, MissingPolicy::Blank)?;
        if operand.missing {
            false
        } else {
            core_projection::coercev2_operand_boolean(&operand.value)?
        }
    } else {
        false
    };
    let case = if length > 4 {
        mode(coercev2_option_number(host, 4, 0.)?)?
    } else {
        Case::Sensitive
    };
    let plan = Split {
        columns: &columns,
        rows: &rows,
        ignore_empty,
        case,
    };
    let work = plan.work(text.len()).map_err(|e| failure(host, e))?;
    host.charge_work(work as u64)?;
    let shape = plan.shape(&text).map_err(|e| failure(host, e))?;
    shape_admission(host, shape.rows, shape.columns)?;
    host.check_values(shape.fields)?;
    host.charge_work(work as u64)?;
    let fields = plan.fields(&text, shape).map_err(|e| failure(host, e))?;
    let padding = if length > 5 {
        let operand = host.scalar_data(5, MissingPolicy::Blank)?;
        if operand.missing {
            Error(CellError::NA)
        } else {
            poirot_coercionv2::compatibility::coercev2_array_member(operand.value)
        }
    } else {
        Error(CellError::NA)
    };
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.size_overflow())?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    let mut fields = fields.into_iter().peekable();
    for row in 0..shape.rows {
        for column in 0..shape.columns {
            if fields
                .peek()
                .is_some_and(|f| f.row == row && f.column == column)
            {
                let field = fields.next().ok_or(CellError::Value)?;
                let selected = text.get(field.span).ok_or(CellError::Value)?;
                admit_text(host, selected.len())?;
                // SAFETY: count is the locally reserved checked row/column product.
                values.push(text_value(selected.to_vec()));
            } else {
                host.check_text(padding.text_ref().map_or(0, |t| t.storage_bytes()))?;
                // SAFETY: count is the locally reserved checked row/column product.
                values.push(padding.clone());
            }
        }
    }
    output(host, values, shape.rows, shape.columns, address)
}
fn serialize_member(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
    strict: bool,
) -> FunctionResult<Vec<u16>> {
    let text = coercev2_text_units(host, value, true)?;
    let quoted = strict && value.text_ref().is_some();
    let count = modern::escaped_len(&text, quoted).map_err(|e| failure(host, e))?;
    if count > 32767 {
        return Err(CellError::Value.into());
    }
    admit_text(host, count)?;
    modern::escape(&text, quoted).map_err(|e| failure(host, e))
}
fn append(host: &mut CallHost<'_, '_>, output: &mut Vec<u16>, text: &[u16]) -> FunctionResult<()> {
    let size = output
        .len()
        .checked_add(text.len())
        .ok_or_else(|| host.size_overflow())?;
    if size > 32767 {
        return Err(CellError::Value.into());
    }
    host.check_text_capacity(size.checked_mul(2).ok_or_else(|| host.size_overflow())?)?;
    host.check_values_capacity(size)?;
    output
        .try_reserve(text.len())
        .map_err(|_| host.allocation_failure())?;
    host.charge_work(text.len() as u64)?;
    // SAFETY: Checked size is at most 32767 and try_reserve admitted all appended units locally.
    output.extend_from_slice(text);
    Ok(())
}
fn serialize(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    let shape = description(host, 0)?;
    if name == TextFunction::ArrayToText {
        let strict = if length > 1 {
            bool_mode(coercev2_option_number(host, 1, 0.)?)?
        } else {
            false
        };
        shape_admission(host, shape.rows, shape.columns)?;
        let mut out = Vec::new();
        if strict {
            append(host, &mut out, &[123])?;
        }
        for row in 0..shape.rows {
            for column in 0..shape.columns {
                if row != 0 || column != 0 {
                    append(
                        host,
                        &mut out,
                        if strict {
                            if column == 0 { &[59] } else { &[44] }
                        } else {
                            &[44, 32]
                        },
                    )?;
                }
                let scalar = host.read_cell(0, row, column)?;
                let text = serialize_member(host, &scalar.value, strict)?;
                append(host, &mut out, &text)?;
            }
        }
        if strict {
            append(host, &mut out, &[125])?;
        }
        return Ok(text_value(out).into());
    }
    let format_shape = if length > 1 {
        Some(description(host, 1)?)
    } else {
        None
    };
    let rows = format_shape.map_or(shape.rows, |f| shape.rows.max(f.rows));
    let columns = format_shape.map_or(shape.columns, |f| shape.columns.max(f.columns));
    shape_admission(host, rows, columns)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(
            rows.checked_mul(columns)
                .ok_or_else(|| host.size_overflow())?,
        )
        .map_err(|_| host.allocation_failure())?;
    for row in 0..rows {
        for column in 0..columns {
            let value = (|| -> FunctionResult<CellValue> {
                let scalar = member(host, 0, shape, row, column)?;
                let strict = if let Some(format_shape) = format_shape {
                    let operand = member(host, 1, format_shape, row, column)?;
                    bool_mode(number(host, operand, 0.)?)?
                } else {
                    false
                };
                Ok(text_value(serialize_member(host, &scalar.value, strict)?))
            })();
            // SAFETY: The local checked rows*columns reservation bounds these pushes.
            values.push(match value {
                Ok(value) => value,
                Err(FunctionFailure::Spreadsheet(error)) => Error(error),
                Err(error) => return Err(error),
            });
        }
    }
    output(host, values, rows, columns, address)
}
/// Evaluate modern text functions with typed array results and operational failures.
pub(crate) fn execute(
    name: TextFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let length = args.len();
    let valid = if matches!(name, TextFunction::ValueToText | TextFunction::ArrayToText) {
        (1..=2).contains(&length)
    } else {
        (2..=6).contains(&length)
    };
    if !valid {
        return Ok(err(CellError::Value));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, length);
    let result = match name {
        TextFunction::TextBefore | TextFunction::TextAfter => {
            extraction(&mut host, name, length, address)
        }
        TextFunction::TextSplit => split(&mut host, length, address),
        TextFunction::ValueToText | TextFunction::ArrayToText => {
            serialize(&mut host, name, length, address)
        }
        _ => Err(CellError::Value.into()),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}

#[cfg(test)]
mod resource_mapping_tests {
    use super::*;
    #[test]
    fn text_size_allocation_and_stale_geometry_are_distinct_operational_failures() {
        let book = Workbook::new();
        let mut session = Session::with_context(&book, Default::default());
        let address = CellAddress { row: 2, col: 3 };
        let host = CallHost::new_array(&mut session, 0, address, ArgumentsRef::Empty, 0);
        for (error, expected) in [
            (
                modern::Error::SizeOverflow,
                ResourceFailureKind::SizeOverflow,
            ),
            (
                modern::Error::AllocationFailure,
                ResourceFailureKind::Allocation,
            ),
        ] {
            assert!(
                matches!(failure(&host, error), FunctionFailure::Operational(EvaluationFailure::Resource { kind, source: Some(source), .. }) if kind == expected && source == EvaluationTarget { sheet: 0, address })
            );
        }
        assert!(matches!(
            failure(&host, modern::Error::InvalidGeometry),
            FunctionFailure::Operational(EvaluationFailure::StalePlan)
        ));
    }
}
