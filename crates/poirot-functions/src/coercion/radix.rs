//! BASE/DECIMAL admission; no engineering signed-width or Java grammar rules.
use crate::coercion::CellError;
use crate::numeric::radix::Radix;
use crate::scalar::Scalar;

/// Numeric BASE/radix/padding arguments accept numbers, context-parsed numeric text,
/// logicals and blanks identically for direct, referenced and array scalars.
/// Omitted syntax is explicitly admitted as zero. The host budgets text first.
pub fn coerce_radix_number<E: From<CellError>>(
    value: &Scalar,
    mut text_number: impl FnMut(&str) -> Result<f64, E>,
) -> Result<f64, E> {
    use crate::coercion::compatibility::{DeferredNumber, NumberPolicy, coerce_number};
    coerce_number(
        value.into(),
        NumberPolicy::Finite,
        |deferred| match deferred {
            DeferredNumber::Missing => Ok(0.0),
            DeferredNumber::Text(crate::coercion::TextRef::Utf8(text)) => text_number(text),
            DeferredNumber::Text(_) => Err(CellError::Value.into()),
        },
    )
}
/// Truncate fractional radices before checking the documented 2..=36 interval.
pub fn coerce_radix(value: f64) -> Result<Radix, CellError> {
    let value = value.trunc();
    if !value.is_finite() || !(2.0..=36.0).contains(&value) {
        return Err(CellError::Num);
    }
    Radix::new(value as u8)
}
/// BASE's unsigned exact-integer interval; negativity is checked before truncation.
pub fn coerce_base_number(value: f64) -> Result<u64, CellError> {
    if !value.is_finite() || !(0.0..9007199254740992.0).contains(&value) {
        return Err(CellError::Num);
    }
    Ok(value.trunc() as u64)
}
/// BASE padding is bounded before any output allocation. Fractional positive
/// counts truncate; negative counts fail, including negative fractions.
pub fn coerce_base_padding(value: f64) -> Result<usize, CellError> {
    if !value.is_finite() || value < 0.0 || value.trunc() > 255.0 {
        return Err(CellError::Num);
    }
    Ok(value.trunc() as usize)
}
/// Required text allocation admission before DECIMAL scalar spelling. Numeric
/// inputs retain the shared source formatter's documented conservative bound.
pub fn coerce_decimal_text_capacity(value: &Scalar) -> usize {
    match value {
        Scalar::Text(text) => text.len(),
        Scalar::Number(_) => crate::numeric::number_text::ALLOCATION_BOUND,
        Scalar::Boolean(_) => 1,
        _ => 0,
    }
}
/// DECIMAL's scalar text argument. Numeric spelling follows spreadsheet decimal
/// text; logicals are the digits 1/0 and explicit blank/missing values are empty.
/// All origins use this same consumer policy; errors preserve their identity.
pub fn coerce_decimal_scalar(value: &Scalar) -> Result<String, CellError> {
    match value {
        Scalar::Text(text) => Ok(text.clone()),
        Scalar::Number(n) if n.is_finite() => {
            Ok(crate::coercion::compatibility::coerce_number_to_text(*n))
        }
        Scalar::Number(_) => Err(CellError::Num),
        Scalar::Boolean(b) => Ok(if *b { "1" } else { "0" }.into()),
        Scalar::Blank | Scalar::Missing => Ok(String::new()),
        Scalar::Error(error) => Err(*error),
    }
}
/// Parse at most 255 UTF-16 units of unsigned positional ASCII text. Leading
/// ASCII spaces are admitted (Calc observation); trailing spaces, signs,
/// prefixes and non-ASCII digits are not grammar extensions. Empty text is zero.
/// Finite values above 2^53 are permitted with binary64 precision loss.
pub fn coerce_decimal_text(text: &str, radix: Radix) -> Result<f64, CellError> {
    if text.encode_utf16().count() > 255 {
        return Err(CellError::Num);
    }
    let text = text.trim_start_matches(' ');
    let mut value = 0.0;
    for byte in text.bytes() {
        value = value * f64::from(radix.value()) + f64::from(radix.digit(byte)?);
        if !value.is_finite() {
            return Err(CellError::Num);
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radix_domains_truncate_and_preserve_documented_bounds() {
        for (n, expected) in [
            (2.9, Ok(2)),
            (36.9, Ok(36)),
            (1.9, Err(CellError::Num)),
            (37.0, Err(CellError::Num)),
        ] {
            assert_eq!(coerce_radix(n).map(Radix::value), expected);
        }
        assert_eq!(coerce_base_number(1.9), Ok(1));
        for n in [-0.1, -1.0, 9007199254740992.0, f64::INFINITY, f64::NAN] {
            assert_eq!(coerce_base_number(n), Err(CellError::Num));
        }
        assert_eq!(coerce_base_padding(255.9), Ok(255));
        for n in [-0.1, 256.0, 1e99] {
            assert_eq!(coerce_base_padding(n), Err(CellError::Num));
        }
    }
    #[test]
    fn decimal_grammar_is_unsigned_case_insensitive_and_finite() {
        let r = Radix::new(36).unwrap();
        for (text, expected) in [
            ("zap", 45745.0),
            ("ZAP", 45745.0),
            ("", 0.0),
            (" ", 0.0),
            (" F", 15.0),
            ("0xF", 1203.0),
        ] {
            assert_eq!(coerce_decimal_text(text, r), Ok(expected));
        }
        for text in ["+F", "-F", "F ", "1.0", "０", "\tF"] {
            assert_eq!(coerce_decimal_text(text, r), Err(CellError::Num));
        }
        assert_eq!(
            coerce_decimal_text("2", Radix::new(2).unwrap()),
            Err(CellError::Num)
        );
        assert_eq!(coerce_decimal_text(&"0".repeat(255), r), Ok(0.0));
        assert_eq!(
            coerce_decimal_text(&"0".repeat(256), r),
            Err(CellError::Num)
        );
        assert!(
            coerce_decimal_text(&"Z".repeat(198), r)
                .unwrap()
                .is_finite()
        );
        assert_eq!(
            coerce_decimal_text(&"Z".repeat(199), r),
            Err(CellError::Num)
        );
        assert_eq!(
            coerce_decimal_text("9007199254740992", Radix::new(10).unwrap()),
            Ok(9007199254740992.0)
        );
    }
}
