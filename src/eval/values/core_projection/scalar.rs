//! Scalar Unicode projections have an operational failure boundary.
//! UTF16 unit consumers must use text_ref directly rather than this scalar view.

use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::eval::values::function_error::FunctionError;
use crate::eval::values::function_result::FunctionResult;
use crate::eval::values::source_arithmetic;
use crate::eval::values::value::VALUE_ERROR;
use crate::model::CellError::{self, Num};
use crate::model::CellValue::{self, Blank, Boolean, Error, Number, Text};
use crate::model::TextRef;
use std::{borrow::Cow, cmp::Ordering};

fn unsupported(reason: crate::eval::limits::EvaluationReasonError) -> FunctionError {
    EvaluationError::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        target: None,
        reason,
    }
    .into()
}

/// Only callers that need Unicode scalar semantics request this projection.
/// Admission precedes every UTF16 -> UTF8 allocation; no replacement is used.
pub(in crate::eval) fn checked_text_view<'a>(
    text: TextRef<'a>,
    budget: &mut BudgetMeter,
) -> FunctionResult<Cow<'a, str>> {
    match text {
        TextRef::Utf8(text) => {
            budget.text(text.len())?;
            budget.charge(WorkKind::TextByte, text.len() as u64)?;
            Ok(Cow::Borrowed(text))
        }
        TextRef::Utf16(units) => {
            budget.charge(WorkKind::TextByte, units.len() as u64)?;
            let bytes = TextRef::Utf16(units).utf8_len().map_err(|_| {
                unsupported(crate::eval::limits::EvaluationReasonError::UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates)
            })?;
            budget.text(bytes)?;
            budget.allocation(units.len())?;
            budget.charge(WorkKind::TextByte, bytes as u64)?;
            String::from_utf16(units).map(Cow::Owned).map_err(|_| {
                unsupported(crate::eval::limits::EvaluationReasonError::UnicodeScalarConsumerCannotProjectIsolatedUtf16Surrogates)
            })
        }
    }
}

/// Compare scalars using source-compatible typed ordering and numeric precision.
pub(in crate::eval) fn source_compare(
    a: &CellValue,
    b: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<Ordering> {
    if let Error(error) = a {
        return Err((*error).into());
    }
    if let Error(error) = b {
        return Err((*error).into());
    }
    // Type dispatch precedes numeric special-exponent validation.
    if matches!(a, Number(_) | Blank) && matches!(b, Number(_) | Blank) {
        let (a, b) = (
            coerce_arithmetic_number(a, budget)?,
            coerce_arithmetic_number(b, budget)?,
        );
        if a == 0.0 && b == 0.0 {
            return Ok(b.is_sign_negative().cmp(&a.is_sign_negative()));
        }
        return source_arithmetic::compare(a, b, budget);
    }
    if matches!(a, Blank) || matches!(b, Blank) {
        return compare(a, b, budget);
    }
    if rank(a) != rank(b) {
        return Ok(rank(a).cmp(&rank(b)));
    }
    compare(a, b, budget)
}

/// Coerce a scalar to a number under shared metered conversion rules.
pub(in crate::eval) fn coerce_arithmetic_number(
    v: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<f64> {
    use poirot_functions::coercion::compatibility::{
        DeferredNumber, NumberPolicy, coerce_arithmetic_text, coerce_number,
    };
    coerce_number(v.into(), NumberPolicy::Finite, |input| match input {
        DeferredNumber::Text(t) => {
            coerce_arithmetic_text(&checked_text_view(t, budget)?).map_err(Into::into)
        }
        DeferredNumber::Missing => Err(VALUE_ERROR.into()),
    })
}

/// Boolean text predicates operate on units; malformed UTF16 is an ordinary
/// nonboolean string here, not a failed scalar projection.
pub(in crate::eval) fn coerce_logical_boolean(v: &CellValue) -> Result<bool, CellError> {
    poirot_functions::coercion::compatibility::coerce_boolean(
        v.into(),
        poirot_functions::coercion::compatibility::BooleanPolicy {
            reject_nan: false,
            blank: true,
        },
    )?
    .ok_or(VALUE_ERROR)
}

/// Source OperandResolver consumers reject NaN; lookup range flags keep coerce_logical_boolean().
pub(in crate::eval) fn coerce_operand_boolean(v: &CellValue) -> Result<bool, CellError> {
    poirot_functions::coercion::compatibility::coerce_boolean(
        v.into(),
        poirot_functions::coercion::compatibility::BooleanPolicy {
            reject_nan: true,
            blank: true,
        },
    )?
    .ok_or(VALUE_ERROR)
}

/// Render a scalar as checked helper text without replacing isolated UTF-16 surrogates.
pub(in crate::eval) fn coerce_formula_text(
    v: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<String> {
    if let Some(text) = v.text_ref() {
        let text = checked_text_view(text, budget)?;
        // Borrowed UTF8 clones only after the same checked bound. Owned projected
        // buffers transfer ownership without making another copy.
        return Ok(text.into_owned());
    }
    // Scalar formatting is bounded by binary64 precision; admit the actual
    // output rather than charging the maximum internal formatter workspace.
    budget.charge(WorkKind::HelperOperation, 1)?;
    let rendered =
        poirot_functions::coercion::compatibility::coerce_text_with(v.into(), |request| {
            match request {
                poirot_functions::coercion::compatibility::DeferredText::Number(n) => {
                    Ok(poirot_functions::coercion::compatibility::coerce_number_to_text(n))
                }
                _ => Err(FunctionError::from(VALUE_ERROR)),
            }
        })?;
    budget.text(rendered.len())?;
    Ok(rendered)
}

fn rank(v: &CellValue) -> u8 {
    match v {
        Number(_) | Blank => 0,
        Text(_) | CellValue::TextUtf16(_) => 1,
        Boolean(_) => 2,
        Error(_) => 0,
    }
}

/// Compare scalar values under the shared spreadsheet comparison contract.
pub(in crate::eval) fn compare(
    a: &CellValue,
    b: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<Ordering> {
    if matches!(a, Number(n) if !n.is_finite()) || matches!(b, Number(n) if !n.is_finite()) {
        return Err(Num.into());
    }
    if let Error(e) = a {
        return Err((*e).into());
    }
    if let Error(e) = b {
        return Err((*e).into());
    }
    // Blank normalization uses borrowed values rather than cloning operand text.
    let zero = Number(0.);
    let empty = Text(String::new());
    let false_value = Boolean(false);
    let a = coerce_comparison_blank(a, b, &zero, &empty, &false_value);
    let b = coerce_comparison_blank(b, a, &zero, &empty, &false_value);
    if let (Some(a), Some(b)) = (a.text_ref(), b.text_ref()) {
        budget.check_text(a.storage_bytes())?;
        budget.check_text(b.storage_bytes())?;
        let units = a
            .storage_bytes()
            .checked_add(b.storage_bytes())
            .ok_or_else(|| {
                unsupported(crate::eval::limits::EvaluationReasonError::ComparisonWorkSizeOverflow)
            })?;
        budget.charge(WorkKind::Comparison, units as u64)?;
        return Ok(poirot_functions::text_units::fold::compare_utf16(
            a.units(),
            b.units(),
        ));
    }
    Ok(match (a, b) {
        (Number(a), Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Boolean(a), Boolean(b)) => a.cmp(b),
        _ => rank(a).cmp(&rank(b)),
    })
}

/// EvalResult callers retain their semantic match arms; operational failures
/// escape before IFERROR/ISERROR and cannot become a cell value.
pub(in crate::eval) fn semantic<T>(result: FunctionResult<T>) -> EvalResult<Result<T, CellError>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(FunctionError::Spreadsheet(error)) => Ok(Err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}

/// Format syntax is semantic; fidelity and renderer resource causes are typed
/// operational failures. Renderer errors never invent BudgetError amounts.
pub(in crate::eval) fn format_failure(
    error: poirot_functions::format::FormatError,
) -> FunctionError {
    use poirot_functions::format::FormatErrorKind::{
        InvalidFormat, ResourceLimit, UnsupportedFidelity,
    };
    match error.kind {
        InvalidFormat => VALUE_ERROR.into(),
        UnsupportedFidelity => EvaluationError::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            target: None,
            reason: crate::eval::limits::EvaluationReasonError::Format(std::sync::Arc::new(error)),
        }
        .into(),
        ResourceLimit => EvaluationError::ResourceLimit {
            capability: CapabilityId::Formatting,
            target: None,
            reason: crate::eval::limits::EvaluationReasonError::Format(std::sync::Arc::new(error)),
        }
        .into(),
    }
}

/// Comparison converts blanks using the counterpart's type, without parsing text.
fn coerce_comparison_blank<'a>(
    value: &'a CellValue,
    counterpart: &CellValue,
    zero: &'a CellValue,
    empty: &'a CellValue,
    false_value: &'a CellValue,
) -> &'a CellValue {
    poirot_functions::coercion::compatibility::coerce_comparison_blank(
        value,
        counterpart,
        zero,
        empty,
        false_value,
    )
}
