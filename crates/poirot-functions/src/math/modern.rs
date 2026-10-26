//! Independently written mathematical identities for documented Excel utilities.
//! Contracts: <https://support.microsoft.com/en-us/excel/functions/acot-function>
//! and the corresponding ACOTH/COT/COTH/CSC/CSCH/SEC/SECH function pages.
use super::{MathResult, finite};
use crate::scalar::CellError;

/// Principal inverse cotangent in the closed binary64 interval from zero to pi.
pub fn acot(value: f64) -> MathResult {
    if !value.is_finite() {
        return Err(CellError::Num);
    }
    finite(1.0_f64.atan2(value))
}
/// Inverse hyperbolic cotangent for real inputs outside [-1, 1].
pub fn acoth(value: f64) -> MathResult {
    if !value.is_finite() || value.abs() <= 1.0 {
        return Err(CellError::Num);
    }
    finite((1.0 / value).atanh())
}
/// Reciprocal trigonometric operation, with Excel's common argument bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reciprocal {
    /// Circular cotangent.
    Cot,
    /// Hyperbolic cotangent.
    Coth,
    /// Circular cosecant.
    Csc,
    /// Hyperbolic cosecant.
    Csch,
    /// Circular secant.
    Sec,
    /// Hyperbolic secant.
    Sech,
}
/// Evaluate a reciprocal without overflowing a finite hyperbolic intermediate.
pub fn reciprocal(value: f64, operation: Reciprocal) -> MathResult {
    if !value.is_finite() || value.abs() >= 134_217_728.0 {
        return Err(CellError::Num);
    }
    let denominator = match operation {
        Reciprocal::Cot => value.tan(),
        Reciprocal::Coth => value.tanh(),
        Reciprocal::Csc => value.sin(),
        Reciprocal::Sec => value.cos(),
        Reciprocal::Csch | Reciprocal::Sech => {
            if value == 0.0 && operation == Reciprocal::Csch {
                return Err(CellError::Div0);
            }
            let e = (-value.abs()).exp();
            let result = if operation == Reciprocal::Sech {
                2.0 * e / (1.0 + e * e)
            } else if value.abs() < 1.0 {
                // sinh is stable near zero; exp(-x)^2 would cancel in the denominator.
                return finite(1.0 / value.sinh());
            } else {
                value.signum() * 2.0 * e / (1.0 - e * e)
            };
            return finite(result);
        }
    };
    if denominator == 0.0 {
        return Err(CellError::Div0);
    }
    finite(1.0 / denominator)
}

/// Incremental multinomial calculation that multiplies exact integer binomial factors into a floating result.
/// It retains the cumulative count instead of collecting every input, and meters the recurrence steps.
#[derive(Clone, Debug)]
pub struct Multinomial {
    sum: bigdecimal::num_bigint::BigUint,
    result: f64,
}
impl Default for Multinomial {
    fn default() -> Self {
        Self::new()
    }
}
impl Multinomial {
    /// Start the empty factorial ratio at one.
    pub fn new() -> Self {
        Self {
            sum: 0_u32.into(),
            result: 1.0,
        }
    }
    /// Add one nonnegative count, charging each bounded exact recurrence step.
    pub fn push<E: From<CellError>>(
        &mut self,
        value: f64,
        mut visit: impl FnMut() -> Result<(), E>,
    ) -> Result<(), E> {
        use bigdecimal::num_traits::FromPrimitive;
        if !value.is_finite() || value < 0.0 {
            return Err(CellError::Num.into());
        }
        let count =
            bigdecimal::num_bigint::BigUint::from_f64(value.trunc()).ok_or(CellError::Num)?;
        let factor = super::binomial_from_counts(self.sum.clone(), count.clone(), &mut visit)?;
        self.result = finite(self.result * factor)?;
        self.sum += count;
        Ok(())
    }
    /// Return the accumulated finite factorial ratio.
    pub fn finish(self) -> f64 {
        self.result
    }
}
