//! Numeric result construction under the pinned policy.

use crate::eval::limits::EvalResult;
use crate::model::{CellError, CellValue};

/// Apply selected non-finite and subnormal number construction behavior.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn convert_numeric_outcome(number: f64) -> CellValue {
    CellValue::Number(if number != 0.0 && number.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        number
    })
}

/// Construct a parsed numeric token, rejecting non-finite pinned literals.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn convert_numeric_literal(number: f64) -> EvalResult<CellValue> {
    if !number.is_finite() {
        return Ok(CellValue::Error(CellError::Num));
    }
    // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
    Ok(convert_numeric_outcome(number))
}

/// Require a finite result before applying policy-specific construction.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn convert_checked_numeric_result(number: f64) -> CellValue {
    if number.is_finite() {
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        convert_numeric_outcome(number)
    } else {
        CellValue::Error(CellError::Num)
    }
}
