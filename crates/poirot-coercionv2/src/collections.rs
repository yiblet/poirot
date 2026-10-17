//! Per-member policies; traversal, shape validation, and allocation stay with hosts.
use crate::compatibility::{DeferredNumber, NumberPolicy, ScalarRef, coercev2_number};
use crate::{CellError, Origin};
/// Source matrix area admission policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixCollection {
    /// Reject blanks, ignore text and booleans.
    Determinant,
    /// Ignore nonnumeric cells, compact then pad the original rectangle.
    Inverse,
    /// Numeric members for matrix multiplication.
    Multiply,
    /// Treat blanks as zero, ignore text and booleans.
    Transpose,
}
/// Admit one matrix area member without reading any other member.
pub fn coercev2_matrix_member(
    value: ScalarRef<'_>,
    policy: MatrixCollection,
) -> Result<Option<f64>, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(Some(n)),
        ScalarRef::Error(e) => Err(e),
        ScalarRef::Blank if policy == MatrixCollection::Determinant => Err(CellError::Value),
        ScalarRef::Blank if policy == MatrixCollection::Transpose => Ok(Some(0.0)),
        _ => Ok(None),
    }
}
/// TREND accepts numeric and logical members; all other kinds yield VALUE.
pub fn coercev2_trend_member(value: ScalarRef<'_>) -> Result<f64, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(n),
        ScalarRef::Boolean(b) => Ok(f64::from(u8::from(b))),
        _ => Err(CellError::Value),
    }
}
/// SUMPRODUCT retains source scalar-versus-area blank/text handling.
pub fn coercev2_sumproduct_member(value: ScalarRef<'_>, scalar: bool) -> Result<f64, CellError> {
    match value {
        ScalarRef::Number(n) => Ok(n),
        ScalarRef::Boolean(b) => Ok(f64::from(u8::from(b))),
        ScalarRef::Error(e) => Err(e),
        ScalarRef::Text(_) | ScalarRef::Blank if !scalar => Ok(0.0),
        _ => Err(CellError::Value),
    }
}
/// Financial cash flows admit direct text/logical inputs and ignore referenced ones.
pub fn coercev2_cashflow_member<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    origin: Origin,
    parse: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    match value {
        ScalarRef::Number(_) | ScalarRef::Error(_) => {
            coercev2_number(value, NumberPolicy::Finite, parse).map(Some)
        }
        ScalarRef::Boolean(_) | ScalarRef::Text(_) if origin == Origin::Direct => {
            coercev2_number(value, NumberPolicy::Finite, parse).map(Some)
        }
        _ => Ok(None),
    }
}
/// FREQUENCY applies the same origin filtering while preserving nonfinite numbers.
pub fn coercev2_frequency_member<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    origin: Origin,
    parse: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<Option<f64>, E> {
    match value {
        ScalarRef::Number(_) | ScalarRef::Error(_) => {
            coercev2_number(value, NumberPolicy::Preserve, parse).map(Some)
        }
        ScalarRef::Boolean(_) | ScalarRef::Text(_) if origin == Origin::Direct => {
            coercev2_number(value, NumberPolicy::Preserve, parse).map(Some)
        }
        _ => Ok(None),
    }
}

/// Numeric aggregates count a directly computed blank as a numeric observation;
/// blanks selected from references or arrays remain blank and can be ignored.
/// Syntactically omitted arguments must retain omission metadata at the host.
pub fn coercev2_aggregate_argument(value: crate::CellValue, origin: Origin) -> crate::CellValue {
    if origin == Origin::Direct && matches!(value, crate::CellValue::Blank) {
        crate::CellValue::Number(0.)
    } else {
        value
    }
}
