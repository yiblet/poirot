//! Field admission, header lookup, and POI cell representation policies.
use super::{CellError, CellValue, DatabaseError, DatabaseFunction, Grid, number};

/// Project a scalar into the numeric channel accepted by database aggregates.
pub fn numeric(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Number(n) => Some(*n),
        CellValue::Boolean(b) => Some(u8::from(*b) as f64),
        _ => None,
    }
}
/// Convert a cell to owned header/criteria text, refusing unsupported UTF-16 projection.
pub fn text(value: &CellValue) -> Result<String, DatabaseError> {
    Ok(match value {
        CellValue::Blank => String::new(),
        CellValue::Text(s) => s.clone(),
        CellValue::TextUtf16(s) => s
            .to_utf8()
            .map_err(|_| DatabaseError::UnsupportedTextProjection)?,
        CellValue::Number(n) => number::to_text(*n),
        CellValue::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        CellValue::Error(e) => return Err((*e).into()),
    })
}
/// Math.round uses nearest integer, with ties towards positive infinity.
/// Adding 0.5 first loses the distinction immediately below a half-integer.
/// Out-of-area indices retain this crate's explicit bounded-input boundary.
pub fn field_index(value: &CellValue, columns: usize) -> Option<usize> {
    let value = numeric(value)?;
    if !value.is_finite() {
        return None;
    }
    let lower = value.floor();
    let rounded = lower + if value - lower >= 0.5 { 1. } else { 0. };
    let column = rounded - 1.;
    if rounded < i32::MIN as f64 || rounded > i32::MAX as f64 {
        return None;
    }
    (column >= 0. && column < columns as f64).then_some(column as usize)
}

/// DStarRunner.getColumnForName applies coerceValueToInt to numeric criteria
/// headings. Unlike the rounded field argument, this floors and checks Java int
/// bounds. The runner converts the resulting EvaluationException to VALUE.
pub fn criteria_header_index(
    value: &CellValue,
    columns: usize,
) -> Result<Option<usize>, CellError> {
    let index = numeric(value).ok_or(CellError::Value)?.floor();
    if !index.is_finite() || index < i32::MIN as f64 || index > i32::MAX as f64 {
        return Err(CellError::Value);
    }
    let column = index - 1.;
    Ok(if column >= 0. && column < columns as f64 {
        Some(column as usize)
    } else {
        None
    })
}
/// Resolve a criteria header against the database's header row.
pub(super) fn header_column(
    database: Grid<'_>,
    name: &str,
) -> Result<Option<usize>, DatabaseError> {
    for col in 0..database.columns {
        let value = database.get(0, col);
        if matches!(value, CellValue::Blank | CellValue::Error(_)) {
            continue;
        }
        if equal_ignore_case(&text(value)?, name) {
            return Ok(Some(col));
        }
    }
    Ok(None)
}
/// Compare header spellings with the family's supported case mapping.
pub fn equal_ignore_case(a: &str, b: &str) -> bool {
    let mut left = a.chars();
    let mut right = b.chars();
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) => {
                let simple_fold = |character: char| {
                    let mut upper = character.to_uppercase();
                    let first = upper.next().unwrap_or(character);
                    let simple_upper = if upper.next().is_some() {
                        character
                    } else {
                        first
                    };
                    simple_upper.to_lowercase().next().unwrap_or(simple_upper)
                };
                let a = simple_fold(a);
                let b = simple_fold(b);
                if a != b {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

/// Validate a field selector and resolve its database column.
pub(super) fn select_field(
    function: DatabaseFunction,
    database: Grid<'_>,
    field: &CellValue,
) -> Result<Option<usize>, DatabaseError> {
    let selected = match field {
        CellValue::Error(error) if !function.empty_field() => return Err((*error).into()),
        CellValue::Error(_) => None,
        value if numeric(value).is_some() => field_index(value, database.columns),
        CellValue::Text(name) => header_column(database, name)?,
        CellValue::Blank => header_column(database, "")?,
        value => header_column(database, &text(value)?)?,
    };
    if selected.is_none() && !function.empty_field() {
        return Err(CellError::Value.into());
    }
    Ok(selected)
}

/// Borrow one selected field, substituting zero for the supported omitted-field count case.
pub(super) fn selected(database: Grid<'_>, field: Option<usize>, row: usize) -> &CellValue {
    // POI's omitted-field count replaces its invalid-column result with zero.
    static ZERO: CellValue = CellValue::Number(0.);
    field.map_or(&ZERO, |col| database.get(row, col))
}
