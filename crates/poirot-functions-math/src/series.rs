//! Direct power-series terms, retaining arbitrary real initial and step exponents.
use super::{MathResult, finite, inputs};

/// Evaluate one term. Zero to zero is one in this consumer; invalid/nonfinite
/// powers remain errors even when multiplied by a zero coefficient.
pub fn term(x: f64, initial: f64, step: f64, index: usize, coefficient: f64) -> MathResult {
    inputs(&[x, initial, step, coefficient])?;
    // TODO(coercion-non-argument): [LOW] Very large series indices can lose integer precision in binary64 exponent arithmetic; host traversal budgets bound practical indices.
    let exponent = finite(initial + index as f64 * step)?;
    let power = finite(x.powf(exponent))?;
    finite(coefficient * power)
}

/// Accumulate in source order, rejecting overflow before later cancellation.
pub fn accumulate(total: f64, term: f64) -> MathResult {
    inputs(&[total, term])?;
    finite(total + term)
}
