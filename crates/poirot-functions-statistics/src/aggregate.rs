use crate::{Aggregate, Result, checked, finite};
use poirot_functions_core::CellError;
use std::collections::HashMap;

/// The slice must already contain values admitted by the function's coercion policy.
/// A variants use the same calculation after their distinct collection policy.
pub fn aggregate(operation: Aggregate, values: &[f64]) -> Result {
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
    let deviation = || values.iter().map(|v| (v - mean).powi(2)).sum::<f64>();
    checked(match operation {
        Aggregate::Average => mean,
        Aggregate::AveDev => values.iter().map(|v| (v - mean).abs()).sum::<f64>() / n as f64,
        Aggregate::DevSq => {
            if n == 1 {
                0.0
            } else {
                deviation()
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
            // StatsLib requires >1 even for population variance/deviation.
            if n < 2 {
                return Err(CellError::Num);
            }
            let sample = matches!(
                operation,
                Aggregate::VarianceSample | Aggregate::StdevSample
            );
            let variance = deviation() / (n - usize::from(sample)) as f64;
            if matches!(
                operation,
                Aggregate::StdevSample | Aggregate::StdevPopulation
            ) {
                variance.sqrt()
            } else {
                variance
            }
        }
        // SAFETY: counts is built from every value in this function and remains unchanged; maximum >= 2 ensures a matching mode exists. Maximum/Minimum/Mode all return before the later aggregate match.
        _ => unreachable!("handled above"),
    })
}
