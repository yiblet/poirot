use crate::{Aggregate, Result, SamplePolicy, SampleRequirement, checked, finite};
use poirot_functions_core::CellError;
use std::collections::HashMap;

/// The slice must already contain values admitted by the function's coercion policy.
/// A variants use the same calculation after their distinct collection policy.
pub fn aggregate(operation: Aggregate, values: &[f64]) -> Result {
    aggregate_with_policy(operation, values, SamplePolicy::PinnedPoi)
}

/// Reduce admitted values under an explicit minimum-observation contract.
pub fn aggregate_with_policy(operation: Aggregate, values: &[f64], policy: SamplePolicy) -> Result {
    finite(values)?;
    let n = values.len();
    if operation == Aggregate::Maximum || operation == Aggregate::Minimum {
        return Ok(values
            .iter()
            .copied()
            .reduce(|a, b| {
                if operation == Aggregate::Maximum {
                    a.max(b)
                } else {
                    a.min(b)
                }
            })
            .unwrap_or(0.0));
    }
    if operation == Aggregate::Mode {
        // Canonicalize signed zero because Java numerical equality treats both as equal.
        let key = |value: f64| if value == 0.0 { 0 } else { value.to_bits() };
        let mut counts = HashMap::<u64, usize>::new();
        for &value in values {
            *counts.entry(key(value)).or_default() += 1;
        }
        let maximum = counts.values().copied().max().unwrap_or(0);
        if maximum < 2 {
            return Err(CellError::NA);
        }
        // SAFETY: counts is built from every value in this function and remains unchanged; maximum >= 2 ensures a matching mode exists. Maximum/Minimum/Mode all return before the later aggregate match.
        return Ok(*values
            .iter()
            // SAFETY: counts is built from every value in this function and remains unchanged; maximum >= 2 ensures a matching mode exists. Maximum/Minimum/Mode all return before the later aggregate match.
            .find(|&&value| counts[&key(value)] == maximum)
            .expect("mode exists"));
    }
    if n == 0 {
        return Err(match operation {
            Aggregate::Average
            | Aggregate::VarianceSample
            | Aggregate::VariancePopulation
            | Aggregate::StdevSample
            | Aggregate::StdevPopulation => CellError::Div0,
            _ => CellError::Num,
        });
    }
    let mean = values.iter().sum::<f64>() / n as f64;

    checked(match operation {
        Aggregate::Average => mean,
        Aggregate::AveDev => values.iter().map(|v| (v - mean).abs()).sum::<f64>() / n as f64,
        Aggregate::DevSq => {
            if n == 1 {
                0.0
            } else {
                let (scale, deviation) = scaled_deviation(values);
                scale * (scale * deviation)
            }
        }
        Aggregate::GeoMean => {
            if values.iter().any(|&v| v <= 0.0) {
                return Err(CellError::Num);
            }
            (values.iter().map(|v| v.ln()).sum::<f64>() / n as f64).exp()
        }
        Aggregate::VarianceSample
        | Aggregate::VariancePopulation
        | Aggregate::StdevSample
        | Aggregate::StdevPopulation => {
            let sample = matches!(
                operation,
                Aggregate::VarianceSample | Aggregate::StdevSample
            );
            SampleRequirement {
                minimum: if policy == SamplePolicy::PinnedPoi || sample {
                    2
                } else {
                    1
                },
                insufficient: if policy == SamplePolicy::PinnedPoi {
                    CellError::Num
                } else {
                    CellError::Div0
                },
            }
            .validate(n)?;
            let (scale, deviation) = scaled_deviation(values);
            let variance = deviation / (n - usize::from(sample)) as f64;
            if matches!(
                operation,
                Aggregate::StdevSample | Aggregate::StdevPopulation
            ) {
                scale * variance.sqrt()
            } else {
                scale * (scale * variance)
            }
        }
        // SAFETY: counts is built from every value in this function and remains unchanged; maximum >= 2 ensures a matching mode exists. Maximum/Minimum/Mode all return before the later aggregate match.
        _ => unreachable!("handled above"),
    })
}

/// Center before scaling so nearby large operands retain their small differences.
fn scaled_deviation(values: &[f64]) -> (f64, f64) {
    let first = values.first().copied().unwrap_or(0.0);
    // Opposite extreme finite values can overflow subtraction; their common
    // center may be zero because their separation already dominates precision.
    let center = if values.iter().all(|value| (value - first).is_finite()) {
        first
    } else {
        0.0
    };
    let scale = values
        .iter()
        .map(|value| (value - center).abs())
        .fold(0.0, f64::max);
    if scale == 0.0 {
        return (0.0, 0.0);
    }
    let count = values.len() as f64;
    let mean = values
        .iter()
        .map(|value| (value - center) / scale)
        .sum::<f64>()
        / count;
    let (squares, residual) = values
        .iter()
        .fold((0.0, 0.0), |(squares, residual), value| {
            let delta = (value - center) / scale - mean;
            (squares + delta * delta, residual + delta)
        });
    (scale, (squares - residual * (residual / count)).max(0.0))
}
