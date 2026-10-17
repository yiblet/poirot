//! Paired operations, forecast and regression execution.
use super::coercionv2::{coercev2_statistics_text_number, work};
use super::collections::{coercev2_statistics_pairs, numeric_only_pairs};
use super::{EvaluationPort, Failure, SemanticsPolicy, StatisticsFunction};
use domain::{PairCollection as PC, PairOperation as Q};
use poirot_functions_statistics as domain;

/// Execute paired statistics and regression under selected collection semantics.
pub(super) fn execute<P: EvaluationPort>(
    p: &mut P,
    f: StatisticsFunction,
    args: &[P::Argument],
    semantics: SemanticsPolicy,
) -> Result<f64, Failure<P::Operational>> {
    use StatisticsFunction::*;
    let result = match f {
        Correl | Pearson | Covar | CovarianceP | CovarianceS | SumX2MY2 | SumX2PY2 | SumXMY2
        | Intercept | Slope | Forecast | ForecastLinear => {
            let forecasting = matches!(f, Forecast | ForecastLinear);
            let number = if forecasting {
                Some(coercev2_statistics_text_number(
                    p,
                    args.first().ok_or(poirot_model::CellError::Value)?,
                )?)
            } else {
                None
            };
            // Every statistical observation consists of two existing numeric coordinates.
            // Text/logical inputs cannot add observations or enter the means' divisor.
            let policy = PC::NumericOnly;
            let (a, b) = if forecasting {
                (
                    args.get(1).ok_or(poirot_model::CellError::Value)?,
                    args.get(2).ok_or(poirot_model::CellError::Value)?,
                )
            } else {
                (
                    args.first().ok_or(poirot_model::CellError::Value)?,
                    args.get(1).ok_or(poirot_model::CellError::Value)?,
                )
            };
            let v = if matches!(f, Intercept | Slope | SumX2MY2 | SumX2PY2 | SumXMY2) {
                numeric_only_pairs(p, a, b, matches!(f, Intercept | Slope))?
            } else {
                coercev2_statistics_pairs(p, a, b, policy)?
            };
            work(p, (v.original_len as u64).saturating_mul(12))?;
            if let Some(n) = number {
                domain::forecast(n, &v.y, &v.x)
            } else if matches!(f, Intercept | Slope) {
                // Preserve known_y/known_x acquisition order, then use the admitted sample size.
                let r = domain::regression(&v.y, &v.x, v.sample_len()?)
                    .map_err(Failure::Spreadsheet)?;
                Ok(if f == Slope { r.slope } else { r.intercept })
            } else {
                let op = match f {
                    Correl | Pearson => Q::Correlation,
                    Covar | CovarianceP => Q::CovariancePopulation,
                    CovarianceS => Q::CovarianceSample,
                    SumX2MY2 => Q::SumX2MinusY2,
                    SumX2PY2 => Q::SumX2PlusY2,
                    SumXMY2 => Q::SumXMinusYSquared,
                    _ => return Err(poirot_model::CellError::Value.into()),
                };
                domain::paired_with_policy(
                    op,
                    &v.x,
                    &v.y,
                    match semantics {
                        SemanticsPolicy::ExcelDocumentation => {
                            domain::SamplePolicy::ExcelDocumentation
                        }
                        SemanticsPolicy::PinnedPoi => domain::SamplePolicy::PinnedPoi,
                    },
                )
            }
        }
        _ => return Err(poirot_model::CellError::Value.into()),
    };
    result.map_err(Failure::Spreadsheet)
}
