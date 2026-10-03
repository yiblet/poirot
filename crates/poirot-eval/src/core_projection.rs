//! Scalar Unicode projections have an operational failure boundary.
//! UTF16 unit consumers must use text_ref directly rather than this scalar view.
use super::*;
use poirot_model::TextRef;
use std::{borrow::Cow, cmp::Ordering};

fn unsupported(detail: &'static str) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: None,
        detail: detail.into(),
    }
    .into()
}
/// Only callers that need Unicode scalar semantics request this projection.
/// Admission precedes every UTF16 -> UTF8 allocation; no replacement is used.
pub(super) fn checked_text_view<'a>(
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
                unsupported("Unicode scalar consumer cannot project isolated UTF16 surrogates")
            })?;
            budget.text(bytes)?;
            budget.allocation(units.len())?;
            budget.charge(WorkKind::TextByte, bytes as u64)?;
            String::from_utf16(units).map(Cow::Owned).map_err(|_| {
                unsupported("Unicode scalar consumer cannot project isolated UTF16 surrogates")
            })
        }
    }
}
/// Preserve numeric inputs directly and coerce other scalar kinds under shared rules.
pub(super) fn operand_number(v: &CellValue, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    if let Number(number) = v {
        return Ok(*number);
    }
    number(v, budget)
}
/// Compare scalars using source-compatible typed ordering and numeric precision.
pub(super) fn source_compare(
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
        for (argument, value) in [(0, a), (1, b)] {
            if let Number(number) = value
                && !number.is_finite()
            {
                return Err(EvaluationFailure::KnownSourceException {
                    source: None,
                    exception: poirot_model::KnownSourceException::SpecialDoubleComparison {
                        argument,
                        bits: poirot_model::IeeeNumberBits::from_number(*number),
                    },
                }
                .into());
            }
        }
        let value = |v: &CellValue| if let Number(n) = v { *n } else { 0.0 };
        let (a, b) = (value(a), value(b));
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
pub(super) fn number(v: &CellValue, budget: &mut BudgetMeter) -> FunctionResult<f64> {
    if let Some(text) = v.text_ref() {
        let text = checked_text_view(text, budget)?;
        return text
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| VALUE_ERROR.into());
    }
    match v {
        Number(n) if n.is_finite() => Ok(*n),
        Number(_) => Err(Num.into()),
        Blank => Ok(0.),
        Boolean(b) => Ok(u8::from(*b).into()),
        Error(e) => Err((*e).into()),
        Text(_) | CellValue::TextUtf16(_) => Err(VALUE_ERROR.into()),
    }
}
/// Boolean text predicates operate on units; malformed UTF16 is an ordinary
/// nonboolean string here, not a failed scalar projection.
pub(super) fn logical(v: &CellValue) -> Result<bool, CellError> {
    if let Some(text) = v.text_ref() {
        let equal = |expected: &[u8]| {
            let length = match text {
                TextRef::Utf8(t) => t.len(),
                TextRef::Utf16(u) => u.len(),
            };
            length == expected.len()
                && text
                    .units()
                    .zip(expected)
                    .all(|(unit, byte)| unit <= 127 && (unit as u8).eq_ignore_ascii_case(byte))
        };
        return if equal(b"TRUE") {
            Ok(true)
        } else if equal(b"FALSE") {
            Ok(false)
        } else {
            Err(VALUE_ERROR)
        };
    }
    match v {
        Boolean(b) => Ok(*b),
        Number(n) => Ok(*n != 0.),
        Blank => Ok(false),
        Error(e) => Err(*e),
        _ => Err(VALUE_ERROR),
    }
}
/// Source OperandResolver consumers reject NaN; lookup range flags keep logical().
pub(super) fn operand_boolean(
    v: &CellValue,
    policy: NumericOutcomePolicy,
) -> Result<bool, CellError> {
    if policy == NumericOutcomePolicy::PinnedPoi7321209 && matches!(v, Number(n) if n.is_nan()) {
        return Err(VALUE_ERROR);
    }
    logical(v)
}
/// Render a scalar as checked helper text without replacing isolated UTF-16 surrogates.
pub(super) fn text(v: &CellValue, budget: &mut BudgetMeter) -> FunctionResult<String> {
    if let Some(text) = v.text_ref() {
        let text = checked_text_view(text, budget)?;
        // Borrowed UTF8 clones only after the same checked bound. Owned projected
        // buffers transfer ownership without making another copy.
        return Ok(text.into_owned());
    }
    let bytes = match v {
        Number(_) => 512,
        Boolean(_) => 5,
        _ => 0,
    };
    budget.text(bytes)?;
    budget.charge(WorkKind::TextByte, bytes as u64)?;
    Ok(match v {
        Blank => String::new(),
        Number(n) => n.to_string(),
        Boolean(true) => "TRUE".into(),
        Boolean(false) => "FALSE".into(),
        Error(e) => return Err((*e).into()),
        _ => return Err(VALUE_ERROR.into()),
    })
}
fn rank(v: &CellValue) -> u8 {
    match v {
        Number(_) | Blank => 0,
        Text(_) | CellValue::TextUtf16(_) => 1,
        Boolean(_) => 2,
        Error(_) => 0,
    }
}
fn lower(text: &str, budget: &mut BudgetMeter) -> FunctionResult<String> {
    let bytes = text
        .chars()
        .flat_map(char::to_lowercase)
        .map(char::len_utf8)
        .try_fold(0usize, |sum, bytes| sum.checked_add(bytes))
        .ok_or_else(|| unsupported("scalar lowercase output size overflow"))?;
    budget.text(bytes)?;
    budget.allocation(text.chars().count())?;
    budget.charge(WorkKind::TextByte, bytes as u64)?;
    Ok(text.to_lowercase())
}
/// Compare scalar values under the shared spreadsheet comparison contract.
pub(super) fn compare(
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
    let a = if matches!(a, Blank) {
        match rank(b) {
            1 => &empty,
            2 => &false_value,
            _ => &zero,
        }
    } else {
        a
    };
    let b = if matches!(b, Blank) {
        match rank(a) {
            1 => &empty,
            2 => &false_value,
            _ => &zero,
        }
    } else {
        b
    };
    if let (Some(a), Some(b)) = (a.text_ref(), b.text_ref()) {
        let units = a
            .storage_bytes()
            .checked_add(b.storage_bytes())
            .ok_or_else(|| unsupported("comparison work size overflow"))?;
        budget.charge(WorkKind::Comparison, units as u64)?;
        if a == b {
            return Ok(Ordering::Equal);
        }
        if a.units().chain(b.units()).all(|u| u <= 127) {
            // No projection/allocation for source-backed ASCII comparison.
            return Ok(a
                .units()
                .map(|u| (u as u8).to_ascii_lowercase())
                .cmp(b.units().map(|u| (u as u8).to_ascii_lowercase())));
        }
        let a = checked_text_view(a, budget)?;
        let b = checked_text_view(b, budget)?;
        return Ok(lower(&a, budget)?.cmp(&lower(&b, budget)?));
    }
    Ok(match (a, b) {
        (Number(a), Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Boolean(a), Boolean(b)) => a.cmp(b),
        _ => rank(a).cmp(&rank(b)),
    })
}
/// EvalResult callers retain their semantic match arms; operational failures
/// escape before IFERROR/ISERROR and cannot become a cell value.
pub(super) fn semantic<T>(result: FunctionResult<T>) -> EvalResult<Result<T, CellError>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(Err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
/// Replace only spreadsheet failures, preserving operational aborts unchanged.
pub(super) fn remap_semantic(error: FunctionFailure, replacement: CellError) -> FunctionFailure {
    match error {
        FunctionFailure::Spreadsheet(_) => replacement.into(),
        other => other,
    }
}

/// Format syntax is semantic; fidelity and renderer resource causes are typed
/// operational failures. Renderer errors never invent BudgetFailure amounts.
pub(super) fn format_failure(error: poirot_format::FormatError) -> FunctionFailure {
    use poirot_format::FormatErrorKind::*;
    match error.kind {
        InvalidFormat => VALUE_ERROR.into(),
        UnsupportedFidelity => EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            source: None,
            detail: error.to_string(),
        }
        .into(),
        ResourceLimit => EvaluationFailure::ResourceLimit {
            capability: CapabilityId::Formatting,
            source: None,
            detail: error.to_string(),
        }
        .into(),
    }
}

#[cfg(test)]
#[path = "core_projection_tests.rs"]
mod tests;
