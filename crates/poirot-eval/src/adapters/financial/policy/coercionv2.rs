//! Scalar coercion and resolve-before-coerce operand preparation.
use super::{CellError, EvaluationPort, Failure, Scalar};

/// Charge financial helper work while preserving operational failures.
pub(super) fn work<P: EvaluationPort>(p: &mut P, n: u64) -> Result<(), Failure<P::Operational>> {
    p.charge_work(n).map_err(Failure::Operational)
}
fn scalar<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Scalar, Failure<P::Operational>> {
    work(p, 1)?;
    p.resolve_scalar(a)
}
/// Admit text storage and conversion work before processing it.
pub(super) fn text_check<P: EvaluationPort>(
    p: &mut P,
    t: &str,
) -> Result<(), Failure<P::Operational>> {
    p.check_text(t.len()).map_err(Failure::Operational)?;
    work(p, t.len() as u64)
}
/// Coerce a resolved scalar to a finite number under financial argument rules.
pub(super) fn coercev2_financial_scalar_number<P: EvaluationPort>(
    p: &mut P,
    s: Scalar,
) -> Result<f64, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{DeferredNumber, NumberPolicy, coercev2_number};
    coercev2_number((&s).into(), NumberPolicy::Finite, |input| match input {
        DeferredNumber::Missing => p.coercev2_missing_number(),
        DeferredNumber::Text(poirot_functions_core::TextRef::Utf8(t)) => {
            text_check(p, t)?;
            p.coercev2_text_number(t)
        }
        DeferredNumber::Text(_) => Err(CellError::Value.into()),
    })
}
/// Resolve one scalar and apply financial numeric coercion.
pub(super) fn coercev2_financial_argument_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let s = scalar(p, a)?;
    coercev2_financial_scalar_number(p, s)
}
/// Use a default for absent or omitted syntax, otherwise coerce the selected operand.
pub(super) fn coercev2_financial_optional_number<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
    i: usize,
    default: f64,
) -> Result<f64, Failure<P::Operational>> {
    let Some(argument) = args.get(i) else {
        return Ok(default);
    };
    let value = scalar(p, argument)?;
    poirot_coercionv2::compatibility::coercev2_optional_number(value, default, |value| {
        coercev2_financial_scalar_number(p, value)
    })
}
/// Resolve and coerce a financial boolean option without masking operational failures.
pub(super) fn coercev2_financial_boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    let value = scalar(p, a)?;
    if let Scalar::Text(t) = &value {
        text_check(p, t)?;
    }
    poirot_coercionv2::compatibility::coercev2_boolean(
        (&value).into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: false,
            blank: true,
        },
    )?
    .ok_or(Failure::UnsupportedCapability(
        "MissingArgEval boolean conversion",
    ))
}
/// Floor a numeric argument and reject values outside signed 32-bit bounds.
pub(super) fn coercev2_financial_floor_i32<O>(n: f64) -> Result<i32, Failure<O>> {
    poirot_coercionv2::compatibility::coercev2_floor_i32(n).map_err(Into::into)
}
/// Render a scalar as text before applying the financial numeric parser.
pub(super) fn coercev2_financial_text_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{OwnedDeferredText, coercev2_owned_text};
    let value = scalar(p, a)?;
    let t = coercev2_owned_text(value, |request| match request {
        OwnedDeferredText::Number(n) => {
            p.check_text(32).map_err(Failure::Operational)?;
            Ok(poirot_coercionv2::compatibility::coercev2_statistics_number_text(n, false))
        }
        OwnedDeferredText::Text(t) => Ok(t),
        OwnedDeferredText::Missing => Err(Failure::UnsupportedCapability(
            "MissingArgEval string conversion",
        )),
    })?;
    text_check(p, &t)?;
    coercev2_financial_decimal_text(&t).ok_or(CellError::Value.into())
}
/// OperandResolver.parseDouble deliberately excludes Java hex numbers, type
/// suffixes and nonfinite values, and does not attempt date/time parsing.
pub(super) fn coercev2_financial_decimal_text(text: &str) -> Option<f64> {
    poirot_coercionv2::compatibility::coercev2_financial_decimal_text(text)
}
/// IPMT, PPMT and RATE resolve every used operand before numeric coercion.
/// In particular, a later spreadsheet error precedes an earlier invalid text.
pub(super) fn resolve_operands<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
) -> Result<Vec<Scalar>, Failure<P::Operational>> {
    p.check_collection(args.len())
        .map_err(Failure::Operational)?;
    let mut resolved = Vec::with_capacity(args.len());
    for a in args {
        let value = scalar(p, a)?;
        if let Scalar::Error(error) = value {
            return Err(error.into());
        }
        resolved.push(value);
    }
    Ok(resolved)
}
