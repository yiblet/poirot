//! Numeric result construction for spreadsheet evaluation.

use crate::eval::limits::EvalResult;
use crate::model::{CellError, CellValue};

/// Construct a computed number, normalizing subnormal results to zero.
// Normalization adapted from Apache POI NumberEval (Apache-2.0), revision
// 732120980140d5ed64b482c470e0b625cdb1ab15:
// https://github.com/apache/poi/blob/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/eval/NumberEval.java
// Only the subnormal-to-zero construction rule is reused here.
// TODO(coercion-non-argument): [HIGH] Computed-result construction normalizes subnormals; argument admission must remain separate.
pub(in crate::eval) fn convert_numeric_outcome(number: f64) -> CellValue {
    CellValue::Number(if number != 0.0 && number.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        number
    })
}

/// Construct a parsed numeric token, rejecting non-finite numeric literals.
// TODO(coercion-non-argument): [HIGH] Literal construction rejects nonfinite tokens before result normalization.
pub(in crate::eval) fn convert_numeric_literal(number: f64) -> EvalResult<CellValue> {
    if !number.is_finite() {
        return Ok(CellValue::Error(CellError::Num));
    }

    Ok(convert_numeric_outcome(number))
}

/// Construct a finite numeric result or return a numeric error.
// TODO(coercion-non-argument): [HIGH] Finite-result construction maps nonfinite outputs to NUM before normalization.
pub(in crate::eval) fn convert_checked_numeric_result(number: f64) -> CellValue {
    if number.is_finite() {
        convert_numeric_outcome(number)
    } else {
        CellValue::Error(CellError::Num)
    }
}
