//! Streaming aggregation with repeated lazy matching for two-pass statistics.
use super::criteria::CriteriaPlan;
use super::selection::{coerce_database_number, selected};
use super::{CellError, CellValue, DatabaseError, DatabaseFunction, DatabasePlan};

/// Aggregate the records selected by a validated database plan.
pub(super) fn execute(plan: &DatabasePlan<'_>) -> Result<CellValue, DatabaseError> {
    let criteria = CriteriaPlan::new(plan.database, plan.criteria);
    let mut count = 0usize;
    let mut sum = 0.;
    let mut product = 0.;
    let mut extremum: Option<&CellValue> = None;
    let mut unique: Option<&CellValue> = None;
    for row in 1..plan.database.rows {
        if !criteria.matches_row(row)? {
            continue;
        }
        let value = selected(plan.database, plan.field, row);
        if plan.function == DatabaseFunction::Get {
            if !matches!(value, CellValue::Blank) {
                if unique.is_some() {
                    return Ok(CellValue::Error(CellError::Num));
                }
                unique = Some(value);
            }
            continue;
        }
        if plan.function == DatabaseFunction::CountA {
            if !matches!(value, CellValue::Blank) {
                count += 1;
            }
            continue;
        }
        if let Some(number) = coerce_database_number(value) {
            count += 1;
            sum += number;
            product = if count == 1 { number } else { product * number };
            extremum = Some(match extremum {
                None => value,
                Some(old) => {
                    if (plan.function == DatabaseFunction::Min
                        && number < coerce_database_number(old).ok_or(CellError::Value)?)
                        || (plan.function == DatabaseFunction::Max
                            && number > coerce_database_number(old).ok_or(CellError::Value)?)
                    {
                        value
                    } else {
                        old
                    }
                }
            });
        }
    }
    use DatabaseFunction::*;
    if plan.function == Get {
        return Ok(match unique {
            None => CellValue::Error(CellError::Value),
            Some(value) if value.text_ref().is_some_and(|text| text.is_empty()) => {
                CellValue::Error(CellError::Value)
            }
            Some(value) => value.clone(),
        });
    }
    let answer = match plan.function {
        Count | CountA => count as f64,
        Average => {
            if count == 0 {
                0.
            } else {
                sum / count as f64
            }
        }
        Min | Max => return Ok(extremum.cloned().unwrap_or(CellValue::Number(0.))),
        Product => product,
        Sum => sum,
        Stdev | StdevP | Var | VarP => {
            let sample = matches!(plan.function, Stdev | Var);
            if count < if sample { 2 } else { 1 } {
                return Ok(CellValue::Error(CellError::Div0));
            }
            let mean = sum / count as f64;
            let mut squared = 0.;
            for row in 1..plan.database.rows {
                if criteria.matches_row(row)?
                    && let Some(value) =
                        coerce_database_number(selected(plan.database, plan.field, row))
                {
                    squared += (value - mean) * (value - mean);
                }
            }
            let variance = squared / (count - usize::from(sample)) as f64;
            if matches!(plan.function, Stdev | StdevP) {
                variance.sqrt()
            } else {
                variance
            }
        }
        // SAFETY: Get returns in the preceding branch and cannot reach this match arm.
        Get => unreachable!(),
    };
    Ok(if answer.is_finite() {
        CellValue::Number(answer)
    } else {
        CellValue::Error(CellError::Num)
    })
}
