//! Budgeted scalar acquisition and source-specific coercion dialects.
use super::{EvaluationPort, Failure, Scalar};
use poirot_model::CellError;

/// Charge helper work while preserving operational failures.
pub(super) fn work<P: EvaluationPort>(p: &mut P, n: u64) -> Result<(), Failure<P::Operational>> {
    p.charge_work(n).map_err(Failure::Operational)
}
/// Charge and resolve only the requested scalar operand.
pub(super) fn scalar<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<Scalar, Failure<P::Operational>> {
    work(p, 1)?;
    p.resolve_scalar(a)
}
/// Admit text storage and work before conversion.
pub(super) fn text_check<P: EvaluationPort>(
    p: &mut P,
    t: &str,
) -> Result<(), Failure<P::Operational>> {
    p.check_text(t.len()).map_err(Failure::Operational)?;
    work(p, t.len() as u64)
}
fn coercev2_statistics_scalar_number<P: EvaluationPort>(
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
/// Resolve and coerce a finite numeric scalar under statistical argument rules.
pub(super) fn coercev2_statistics_argument_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let s = scalar(p, a)?;
    coercev2_statistics_scalar_number(p, s)
}
/// Resolve and apply pinned statistical boolean coercion.
pub(super) fn coercev2_statistics_boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    let value = scalar(p, a)?;
    coercev2_statistics_scalar_boolean(p, value)
}
/// Coerce a boolean option under documented rules, treating omission as false.
pub(super) fn coercev2_statistics_documented_boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    let value = scalar(p, a)?;
    if let Scalar::Text(text) = &value {
        text_check(p, text)?;
    }
    poirot_coercionv2::compatibility::coercev2_boolean_default(
        (&value).into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: p.pinned_numeric_outcomes(),
            blank: true,
        },
    )
    .map_err(Into::into)
}
fn coercev2_statistics_scalar_boolean<P: EvaluationPort>(
    p: &mut P,
    value: Scalar,
) -> Result<bool, Failure<P::Operational>> {
    if let Scalar::Text(t) = &value {
        text_check(p, t)?;
    }
    poirot_coercionv2::compatibility::coercev2_boolean(
        (&value).into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: p.pinned_numeric_outcomes(),
            blank: true,
        },
    )?
    .ok_or(Failure::UnsupportedCapability(
        "MissingArgEval boolean conversion",
    ))
}
// POI NormDist passes cumulative directly to OperandResolver rather than
// implicitly intersecting a reference. The engine must preserve raw topology.
/// Require a nonblank boolean option, preserving raw reference-topology restrictions.
pub(super) fn coercev2_statistics_required_boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
    raw: bool,
) -> Result<bool, Failure<P::Operational>> {
    if raw && (p.is_reference(a)? || p.is_area(a)?) {
        return Err(Failure::UnsupportedCapability(
            "POI raw boolean reference topology",
        ));
    }
    let v = scalar(p, a)?;
    if let Scalar::Text(t) = &v {
        text_check(p, t)?;
    }
    poirot_coercionv2::compatibility::coercev2_boolean(
        (&v).into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: p.pinned_numeric_outcomes(),
            blank: false,
        },
    )?
    .ok_or(Failure::UnsupportedCapability(
        "MissingArgEval boolean conversion",
    ))
}
/// Floor a numeric argument and reject values outside signed 32-bit bounds.
pub(super) fn coercev2_statistics_floor_i32<O>(n: f64) -> Result<i32, Failure<O>> {
    poirot_coercionv2::compatibility::coercev2_floor_i32(n).map_err(Into::into)
}
/// Render an operand as text before applying the statistical numeric grammar.
pub(super) fn coercev2_statistics_text_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    use poirot_coercionv2::compatibility::{OwnedDeferredText, coercev2_owned_text};
    let value = scalar(p, a)?;
    let t = coercev2_owned_text(value, |request| match request {
        OwnedDeferredText::Number(n) => {
            p.check_text(32).map_err(Failure::Operational)?;
            Ok(
                poirot_coercionv2::compatibility::coercev2_statistics_number_text(
                    n,
                    p.pinned_numeric_outcomes(),
                ),
            )
        }
        OwnedDeferredText::Text(t) => Ok(t),
        OwnedDeferredText::Missing => {
            if p.pinned_numeric_outcomes() {
                return Err(Failure::KnownSourceException(
                    poirot_model::KnownSourceException::MissingArgumentCoercion,
                ));
            }
            Err(Failure::UnsupportedCapability(
                "MissingArgEval string conversion",
            ))
        }
    })?;
    text_check(p, &t)?;
    if p.pinned_numeric_outcomes() {
        coercev2_statistics_decimal_text(&t).ok_or(CellError::Value.into())
    } else {
        poirot_coercionv2::compatibility::coercev2_java_number_text(&t)
            .ok_or(CellError::Value.into())
    }
}
/// Remap only spreadsheet errors, preserving operational and source failures.
pub(super) fn collapse<O>(e: Failure<O>, to: CellError) -> Failure<O> {
    match e {
        Failure::Spreadsheet(_) => Failure::Spreadsheet(to),
        other => other,
    }
}
// OperandResolver.parseDouble excludes Java hex, suffixes, NaN and infinity.
// Collection paths do not use coerceValueToDouble's date-time fallback.
/// Parse finite decimal collection text without hexadecimal or date fallback.
pub(super) fn coercev2_statistics_decimal_text(text: &str) -> Option<f64> {
    poirot_coercionv2::compatibility::coercev2_statistics_decimal_text(text)
}
