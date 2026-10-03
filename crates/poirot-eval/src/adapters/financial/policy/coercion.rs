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
pub(super) fn numeric<P: EvaluationPort>(
    p: &mut P,
    s: Scalar,
) -> Result<f64, Failure<P::Operational>> {
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
/// Resolve one scalar and apply financial numeric coercion.
pub(super) fn number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let s = scalar(p, a)?;
    numeric(p, s)
}
/// Use a default for absent or omitted syntax, otherwise coerce the selected operand.
pub(super) fn optional<P: EvaluationPort>(
    p: &mut P,
    args: &[P::Argument],
    i: usize,
    default: f64,
) -> Result<f64, Failure<P::Operational>> {
    if i >= args.len() {
        return Ok(default);
    }
    let s = scalar(p, args.get(i).ok_or(CellError::Value)?)?;
    if s == Scalar::Missing {
        Ok(default)
    } else {
        numeric(p, s)
    }
}
/// Resolve and coerce a financial boolean option without masking operational failures.
pub(super) fn boolean<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<bool, Failure<P::Operational>> {
    match scalar(p, a)? {
        Scalar::Boolean(b) => Ok(b),
        Scalar::Number(n) => Ok(n != 0.0),
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
/// Floor a numeric argument and reject values outside signed 32-bit bounds.
pub(super) fn int<O>(n: f64) -> Result<i32, Failure<O>> {
    let n = n.floor();
    if n < i32::MIN as f64 || n > i32::MAX as f64 {
        Err(CellError::Num.into())
    } else {
        Ok(n as i32)
    }
}
/// Render a scalar as text before applying the financial numeric parser.
pub(super) fn string_number<P: EvaluationPort>(
    p: &mut P,
    a: &P::Argument,
) -> Result<f64, Failure<P::Operational>> {
    let t = match scalar(p, a)? {
        Scalar::Number(n) => {
            p.check_text(32).map_err(Failure::Operational)?;
            poirot_numeric::to_text(n)
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
            return Err(Failure::UnsupportedCapability(
                "MissingArgEval string coercion",
            ));
        }
    };
    text_check(p, &t)?;
    parse_operand_number(&t).ok_or(CellError::Value.into())
}
/// OperandResolver.parseDouble deliberately excludes Java hex numbers, type
/// suffixes and nonfinite values, and does not attempt date/time parsing.
pub(super) fn parse_operand_number(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let mut digits = at - start;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        digits += at - start;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    if at != bytes.len() {
        return None;
    }
    poirot_numeric::parse_java_number(text).filter(|number| number.is_finite())
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
