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
fn numeric<P: EvaluationPort>(p: &mut P, s: Scalar) -> Result<f64, Failure<P::Operational>> {
    let n = match s {
        Scalar::Number(n) => n,
        Scalar::Boolean(b) => u8::from(b) as f64,
        Scalar::Blank => 0.0,
        Scalar::Missing => p.coerce_missing_number()?,
        Scalar::Error(e) => return Err(e.into()),
        Scalar::Text(t) => {
            text_check(p, &t)?;
            p.coerce_text_number(&t)?
        }
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(CellError::Num.into())
    }
}
/// Resolve and coerce a finite numeric scalar under statistical argument rules.
pub(super) fn number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let s = scalar(p, a)?;
    numeric(p, s)
}
/// Resolve and apply pinned statistical boolean coercion.
pub(super) fn boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    let value = scalar(p, a)?;
    boolean_value(p, value)
}
/// Coerce a boolean option under documented rules, treating omission as false.
pub(super) fn excel_boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    let value = scalar(p, a)?;
    if value == Scalar::Missing {
        return Ok(false);
    }
    boolean_value(p, value)
}
fn boolean_value<P: EvaluationPort>(
    p: &mut P,
    value: Scalar,
) -> Result<bool, Failure<P::Operational>> {
    match value {
        Scalar::Boolean(b) => Ok(b),
        Scalar::Number(n) => {
            if p.pinned_numeric_outcomes() && n.is_nan() {
                Err(CellError::Value.into())
            } else {
                Ok(n != 0.0)
            }
        }
        Scalar::Blank => Ok(false),
        Scalar::Error(e) => Err(e.into()),
        Scalar::Text(t) => {
            text_check(p, &t)?;
            if t.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if t.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Err(CellError::Value.into())
            }
        }
        Scalar::Missing => Err(Failure::UnsupportedCapability(
            "MissingArgEval boolean coercion",
        )),
    }
}
// POI NormDist passes cumulative directly to OperandResolver rather than
// implicitly intersecting a reference. The engine must preserve raw topology.
/// Require a nonblank boolean option, preserving raw reference-topology restrictions.
pub(super) fn required_boolean<P: EvaluationPort>(
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
    match v {
        Scalar::Blank => Err(CellError::Value.into()),
        Scalar::Boolean(b) => Ok(b),
        Scalar::Number(n) => {
            if p.pinned_numeric_outcomes() && n.is_nan() {
                Err(CellError::Value.into())
            } else {
                Ok(n != 0.0)
            }
        }
        Scalar::Error(e) => Err(e.into()),
        Scalar::Text(t) => {
            text_check(p, &t)?;
            if t.eq_ignore_ascii_case("true") {
                Ok(true)
            } else if t.eq_ignore_ascii_case("false") {
                Ok(false)
            } else {
                Err(CellError::Value.into())
            }
        }
        Scalar::Missing => Err(Failure::UnsupportedCapability(
            "MissingArgEval boolean coercion",
        )),
    }
}
/// Floor a numeric argument and reject values outside signed 32-bit bounds.
pub(super) fn int<O>(n: f64) -> Result<i32, Failure<O>> {
    let n = n.floor();
    if n < i32::MIN as f64 || n > i32::MAX as f64 {
        Err(CellError::Num.into())
    } else {
        Ok(n as i32)
    }
}
/// Render an operand as text before applying the statistical numeric grammar.
pub(super) fn string_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let t = match scalar(p, a)? {
        Scalar::Number(n) => {
            p.check_text(32).map_err(Failure::Operational)?;
            if p.pinned_numeric_outcomes() && !n.is_finite() {
                // NumberEval.getStringValue calls public toText, whose
                // Double.doubleToLongBits canonicalizes every NaN payload.
                // Special signs are ignored. parseDouble below rejects the
                // canonical NaN text's overflow; infinity text parses finite.
                if n.is_nan() {
                    "2.6965397022935E+308".into()
                } else {
                    "1.7976931348623E+308".into()
                }
            } else {
                poirot_numeric::to_text(n)
            }
        }
        Scalar::Text(t) => t,
        Scalar::Blank => String::new(),
        Scalar::Boolean(b) => {
            if b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Scalar::Error(e) => return Err(e.into()),
        Scalar::Missing => {
            if p.pinned_numeric_outcomes() {
                return Err(Failure::KnownSourceException(
                    poirot_model::KnownSourceException::MissingArgumentCoercion,
                ));
            }
            return Err(Failure::UnsupportedCapability(
                "MissingArgEval string coercion",
            ));
        }
    };
    text_check(p, &t)?;
    if p.pinned_numeric_outcomes() {
        parse_collection_number(&t).ok_or(CellError::Value.into())
    } else {
        poirot_numeric::parse_java_number(&t).ok_or(CellError::Value.into())
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
pub(super) fn parse_collection_number(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    if text
        .bytes()
        .any(|c| !matches!(c, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}
