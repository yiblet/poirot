use crate::{PairOperation, Regression, Result, SamplePolicy, SampleRequirement, checked, finite};
use poirot_functions_core::CellError;

/// Adapters preserve positions while filtering a pair whenever either member is excluded.
pub fn paired(operation: PairOperation, x: &[f64], y: &[f64]) -> Result {
    paired_with_policy(operation, x, y, SamplePolicy::PinnedPoi)
}
/// Reduce aligned admitted observations under an explicit sample-size contract.
pub fn paired_with_policy(
    operation: PairOperation,
    x: &[f64],
    y: &[f64],
    policy: SamplePolicy,
) -> Result {
    if x.len() != y.len() {
        return Err(CellError::NA);
    }
    finite(x)?;
    finite(y)?;
    if matches!(
        operation,
        PairOperation::SumX2MinusY2 | PairOperation::SumX2PlusY2 | PairOperation::SumXMinusYSquared
    ) {
        if x.is_empty() {
            return Err(CellError::Div0);
        }
        return checked(
            x.iter()
                .zip(y)
                .map(|(&a, &b)| match operation {
                    PairOperation::SumX2MinusY2 => a * a - b * b,
                    PairOperation::SumX2PlusY2 => a * a + b * b,
                    _ => (a - b).powi(2),
                })
                .sum(),
        );
    }
    SampleRequirement {
        minimum: if policy == SamplePolicy::ExcelDocumentation
            && operation == PairOperation::CovariancePopulation
        {
            1
        } else {
            2
        },
        insufficient: if policy == SamplePolicy::ExcelDocumentation
            || operation == PairOperation::Correlation
        {
            CellError::Div0
        } else {
            CellError::NA
        },
    }
    .validate(x.len())?;
    let mx = x.iter().sum::<f64>() / x.len() as f64;
    let my = y.iter().sum::<f64>() / y.len() as f64;
    let cross = x
        .iter()
        .zip(y)
        .map(|(&a, &b)| (a - mx) * (b - my))
        .sum::<f64>();
    if operation == PairOperation::Correlation {
        let xx = x.iter().map(|v| (v - mx).powi(2)).sum::<f64>();
        let yy = y.iter().map(|v| (v - my).powi(2)).sum::<f64>();
        if xx == 0.0 || yy == 0.0 {
            return Err(CellError::Div0);
        }
        checked(cross / (xx * yy).sqrt())
    } else {
        checked(
            cross / (x.len() - usize::from(operation == PairOperation::CovarianceSample)) as f64,
        )
    }
}
/// SLOPE/INTERCEPT divide sums by original vector length, including ignored pairs.
/// FORECAST uses the filtered numeric pair count instead.
pub fn regression(
    x: &[f64],
    y: &[f64],
    original_len: usize,
) -> std::result::Result<Regression, CellError> {
    if x.len() != y.len() || original_len < x.len() {
        return Err(CellError::NA);
    }
    finite(x)?;
    finite(y)?;
    if x.is_empty() {
        return Err(CellError::Div0);
    }
    let mx = x.iter().sum::<f64>() / original_len as f64;
    let my = y.iter().sum::<f64>() / original_len as f64;
    let xx = x.iter().map(|v| (v - mx).powi(2)).sum::<f64>();
    if xx == 0.0 {
        return Err(CellError::Div0);
    }
    let slope = checked(
        x.iter()
            .zip(y)
            .map(|(&a, &b)| (a - mx) * (b - my))
            .sum::<f64>()
            / xx,
    )?;
    Ok(Regression {
        slope,
        intercept: checked(my - slope * mx)?,
    })
}

/// Predict Y from the fitted slope and intercept at the requested X.
pub fn forecast(number: f64, x: &[f64], y: &[f64]) -> Result {
    if !number.is_finite() {
        return Err(CellError::Value);
    }
    let line = regression(x, y, x.len())?;
    checked(line.intercept + line.slope * number)
}
