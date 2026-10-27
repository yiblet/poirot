//! Field admission, header lookup, and POI cell representation policies.
use super::{CellError, CellValue, DatabaseError, DatabaseFunction, Grid};

/// Project a scalar into the numeric channel accepted by database aggregates.
pub fn coerce_database_number(value: &CellValue) -> Option<f64> {
    crate::coercion::compatibility::coerce_database_number(value)
}
/// Convert a cell to owned header/criteria text, refusing unsupported UTF-16 projection.
pub fn coerce_database_text(value: &CellValue) -> Result<String, DatabaseError> {
    use crate::coercion::compatibility::TextCoercionError;
    crate::coercion::compatibility::coerce_text(
        value.into(),
        crate::coercion::compatibility::coerce_number_to_text,
    )
    .map_err(|error| match error {
        TextCoercionError::Worksheet(e) => e.into(),
        TextCoercionError::UnsupportedText | TextCoercionError::Missing => {
            DatabaseError::UnsupportedTextProjection
        }
    })
}
/// Math.round uses nearest integer, with ties towards positive infinity.
/// Adding 0.5 first loses the distinction immediately below a half-integer.
/// Out-of-area indices retain this crate's explicit bounded-input boundary.
pub fn coerce_database_field_index(value: &CellValue, columns: usize) -> Option<usize> {
    crate::coercion::compatibility::coerce_database_field_index(value, columns)
}

/// DStarRunner.getColumnForName applies coerceValueToInt to numeric criteria
/// headings. Unlike the rounded field argument, this floors and checks Java int
/// bounds. The runner converts the resulting EvaluationException to VALUE.
pub fn coerce_database_criteria_header_index(
    value: &CellValue,
    columns: usize,
) -> Result<Option<usize>, CellError> {
    crate::coercion::compatibility::coerce_database_criteria_header_index(value, columns)
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
        if equal_ignore_case(&coerce_database_text(value)?, name) {
            return Ok(Some(col));
        }
    }
    Ok(None)
}
/// Compare header spellings with the family's supported case mapping.
pub fn equal_ignore_case(a: &str, b: &str) -> bool {
    crate::text_units::fold::compare(a, b).is_eq()
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
        value if coerce_database_number(value).is_some() => {
            coerce_database_field_index(value, database.columns)
        }
        CellValue::Text(name) => header_column(database, name)?,
        CellValue::Blank => header_column(database, "")?,
        value => header_column(database, &coerce_database_text(value)?)?,
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
