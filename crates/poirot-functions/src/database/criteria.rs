//! Lazy OR-row/AND-column criteria traversal and database condition matching.
use super::selection::{
    coerce_database_criteria_header_index, coerce_database_number, coerce_database_text,
    equal_ignore_case, header_column,
};
use super::{CellError, CellValue, DatabaseError, Grid, number};
use crate::coercion::compatibility::coerce_java_number_text;

/// Borrows database and criteria grids for lazy record matching.
/// Header resolution and condition errors occur only for visited conditions; criteria rows are ORed and columns within a row are ANDed.
#[derive(Clone, Copy)]
pub(super) struct CriteriaPlan<'a> {
    database: Grid<'a>,
    criteria: Grid<'a>,
}

impl<'a> CriteriaPlan<'a> {
    /// Bind criteria columns to database headers and validate supported conditions.
    pub(super) fn new(database: Grid<'a>, criteria: Grid<'a>) -> Self {
        Self { database, criteria }
    }
    /// Test one database record against the compiled criteria rows.
    pub(super) fn matches_row(&self, row: usize) -> Result<bool, DatabaseError> {
        for cr in 1..self.criteria.rows {
            let mut matches = true;
            for cc in 0..self.criteria.columns {
                let condition = self.criteria.get(cr, cc);
                if matches!(condition, CellValue::Blank) {
                    continue;
                }
                let selected = match self.criteria.get(0, cc) {
                    CellValue::Text(header) => header_column(self.database, header)?,
                    CellValue::TextUtf16(_) => header_column(
                        self.database,
                        &coerce_database_text(self.criteria.get(0, cc))?,
                    )?,
                    header if coerce_database_number(header).is_some() => {
                        coerce_database_criteria_header_index(header, self.database.columns)?
                    }
                    _ => return Err(CellError::Value.into()),
                };
                let Some(column) = selected else {
                    if coerce_database_text(condition)?.is_empty() {
                        return Err(CellError::Value.into());
                    }
                    return Err(DatabaseError::FormulaCriteria);
                };
                if !condition_matches(self.database.get(row, column), condition)? {
                    matches = false;
                    break;
                }
            }
            if matches {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// Compare one database value with a criterion using database coercion rules.
pub fn condition_matches(value: &CellValue, condition: &CellValue) -> Result<bool, DatabaseError> {
    condition_matches_metered(value, condition, &mut || Ok(()))
}
/// Compare a selected record, admitting each wildcard state through the host.
pub fn condition_matches_metered<E: From<DatabaseError>>(
    value: &CellValue,
    condition: &CellValue,
    visit: &mut impl FnMut() -> Result<(), E>,
) -> Result<bool, E> {
    match condition {
        CellValue::Number(_) | CellValue::Boolean(_) => {
            let number = if let Some(number) = coerce_database_number(value) {
                Some(number)
            } else if value.text_ref().is_some() {
                coerce_java_number_text(&coerce_database_text(value)?)
            } else {
                None
            };
            Ok(number == coerce_database_number(condition))
        }
        CellValue::Error(e) => Ok(matches!(value,CellValue::Error(v) if v == e)),
        CellValue::Blank => Ok(true),
        CellValue::Text(condition) => string_condition(value, condition, visit),
        CellValue::TextUtf16(_) => {
            string_condition(value, &coerce_database_text(condition)?, visit)
        }
    }
}
fn string_condition<E: From<DatabaseError>>(
    value: &CellValue,
    condition: &str,
    visit: &mut impl FnMut() -> Result<(), E>,
) -> Result<bool, E> {
    use poirot_parsing::criteria::Comparison::{Eq, Ge, Gt, Le, Lt, Ne, None};
    let (operator, operand) = poirot_parsing::criteria::parse_comparison(condition);
    if operator == None {
        if operand.is_empty() {
            return Ok(value.text_ref().is_some());
        }
        let actual = coerce_database_text(value)?;
        return if crate::criteria::has_wildcards(operand) {
            crate::criteria::wildcard_match_metered(operand, &actual, visit)
        } else {
            let mut actual = actual.chars().map(crate::text_units::fold::simple_fold);
            Ok(operand
                .chars()
                .map(crate::text_units::fold::simple_fold)
                .all(|character| actual.next() == Some(character)))
        };
    }
    if operator == Eq && operand.is_empty() {
        return Ok(matches!(value, CellValue::Blank));
    }
    let parsed = coerce_java_number_text(operand);
    if matches!(operator, Lt | Le | Gt | Ge) || parsed.is_some() {
        let Some(actual) = coerce_database_number(value) else {
            return Ok(false);
        };
        let expected = parsed.ok_or(DatabaseError::Cell(CellError::Value))?;
        let comparison = number::compare(actual, expected).map_err(DatabaseError::from)?;
        return Ok(match operator {
            Lt => comparison.is_lt(),
            Le => !comparison.is_gt(),
            Gt => comparison.is_gt(),
            Ge => !comparison.is_lt(),
            Eq => comparison.is_eq(),
            Ne => !comparison.is_eq(),
            None => false,
        });
    }
    let equal = equal_ignore_case(&coerce_database_text(value)?, operand);
    Ok(if operator == Ne { !equal } else { equal })
}
