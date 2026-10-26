//! Engineering calculations for spreadsheet functions.
//!
//! Includes binary, octal, and hexadecimal conversion, complex-number helpers, and
//! Bessel calculations. Call operations such as [`hex2dec`](crate::engineering::hex2dec), [`dec2bin`](crate::engineering::dec2bin), [`complex()`](crate::engineering::complex()),
//! and [`besselj`](crate::engineering::besselj) with supplied values.
//!
//! The caller resolves workbook references and evaluates arguments. Shared argument
//! parsing lives in [coercion](https://docs.rs/poirot-functions/latest/poirot_functions/coercion).

// Rust adaptations of Apache POI (Apache-2.0); class bindings are described below.
// <https://github.com/apache/poi/tree/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/functions>
// Pure engineering algorithms and retained pinned POI helper APIs.
// Argument parsing and result spelling belong to the coercion module; references,
// resource admission and operational errors belong to the evaluator adapter.

/// Binary64 modified Bessel base approximations.
mod bessel_k;
/// Bounded real special functions and numeric threshold arithmetic.
pub mod special;

/// Stable complex arithmetic and principal transcendental kernels.
pub mod complex;
/// Strict signed engineering radix and checked bitwise arithmetic.
pub mod radix;
/// Physical measurement conversion using explicit unit definitions.
pub mod units;

/// Spreadsheet value/domain failures from engineering conversions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineeringError {
    /// Input spelling or value type is invalid.
    Value,
    /// Input or requested width exceeds the supported numeric range.
    Num,
}
/// Engineering kernel result, with spreadsheet value/domain failures in [`EngineeringError`].
pub type Result<T> = std::result::Result<T, EngineeringError>;

/// Decode an Excel hexadecimal integer, including signed ten-digit notation.
pub fn hex2dec(text: &str) -> Result<f64> {
    base_to_decimal(text, 16)
}
/// Decode an Excel octal integer with its signed-width rules.
pub fn oct2dec(text: &str) -> Result<f64> {
    base_to_decimal(text, 8)
}
fn base_to_decimal(text: &str, base: u32) -> Result<f64> {
    if !(2..=36).contains(&base) {
        return Err(EngineeringError::Num);
    }
    if text.len() > 10 {
        return Err(EngineeringError::Num);
    }
    let mut value = 0u64;
    for ch in text.chars() {
        // SAFETY: The local guard restricts base to 2..=36, within to_digit's supported radix range.
        let digit = ch
            .to_digit(base)
            .filter(|_| ch.is_ascii())
            .ok_or(EngineeringError::Num)?;
        value = value * base as u64 + digit as u64;
    }
    let sign = text.len() == 10
        // SAFETY: The short-circuit condition requires exactly ten bytes before reading the first byte.
        && text.as_bytes()[0] as char != '0'
        && text
            .chars()
            .next()
            // SAFETY: The local guard restricts base to 2..=36, within to_digit's supported radix range.
.and_then(|c| c.to_digit(base))
            .unwrap_or(0)
            >= base / 2;
    Ok(if sign {
        // SAFETY: The local radix check restricts base to 2..=36; even 36^10 fits u64.
        value as f64 - (base as u64).pow(10) as f64
    } else {
        value as f64
    })
}

/// Java's char-based Integer.parseInt accepts BMP decimal digits. Supplementary
/// digits occupy two UTF-16 chars and are rejected by POI's one-char substrings.
fn java_decimal_digit(unit: u16) -> Option<u32> {
    const ZEROS: &[u16] = &[
        0x0030, 0x0660, 0x06f0, 0x07c0, 0x0966, 0x09e6, 0x0a66, 0x0ae6, 0x0b66, 0x0be6, 0x0c66,
        0x0ce6, 0x0d66, 0x0de6, 0x0e50, 0x0ed0, 0x0f20, 0x1040, 0x1090, 0x17e0, 0x1810, 0x1946,
        0x19d0, 0x1a80, 0x1a90, 0x1b50, 0x1bb0, 0x1c40, 0x1c50, 0xa620, 0xa8d0, 0xa900, 0xa9d0,
        0xa9f0, 0xaa50, 0xabf0, 0xff10,
    ];
    ZEROS.iter().find_map(|&zero| {
        let digit = unit.checked_sub(zero)?;
        (digit < 10).then_some(u32::from(digit))
    })
}

/// POI's short positive binary path accepts all Java decimal digits, unlike
/// its negative path, which parses the remaining chars with radix two.
pub fn bin2dec(text: &str) -> Result<f64> {
    // Collect at most eleven chars so a long input cannot cause unbounded allocation.
    let units: Vec<_> = text.encode_utf16().take(11).collect();
    if units.len() > 10 {
        return Err(EngineeringError::Num);
    }
    // SAFETY: The ten-unit branch permits indices zero/one; the plus-prefix branch checks that digits has a first unit.
    let negative = units.len() == 10 && units[0] != u16::from(b'0');
    let mut digits = if units.len() == 10 {
        // SAFETY: The ten-unit branch permits indices zero/one; the plus-prefix branch checks that digits has a first unit.
        &units[1..]
    } else {
        &units[..]
    };
    // Long.parseLong accepts a leading ASCII plus in the negative-path tail.
    if negative && digits.first() == Some(&u16::from(b'+')) {
        // SAFETY: The ten-unit branch permits indices zero/one; the plus-prefix branch checks that digits has a first unit.
        digits = &digits[1..];
    }
    let mut value = 0u64;
    for &unit in digits {
        let digit = java_decimal_digit(unit).ok_or(EngineeringError::Num)?;
        if negative && digit >= 2 {
            return Err(EngineeringError::Num);
        }
        value = value * 2 + u64::from(digit);
    }
    Ok(if negative {
        value as f64 - 512.0
    } else {
        value as f64
    })
}

/// Encode an integer using Excel binary width and sign rules.
pub fn dec2bin(number: f64, places: Option<f64>) -> Result<String> {
    decimal_to_base(number, places, 2, 512)
}
/// Encode an integer using Excel hexadecimal width and sign rules.
pub fn dec2hex(number: f64, places: Option<f64>) -> Result<String> {
    decimal_to_base(number, places, 16, 549_755_813_888)
}
fn decimal_to_base(number: f64, places: Option<f64>, base: u32, half: i64) -> Result<String> {
    if !number.is_finite() || number <= -(half as f64) - 1.0 || number >= half as f64 {
        return Err(EngineeringError::Num);
    }
    let number = number.trunc() as i64;
    let unsigned = if number < 0 {
        number + 2 * half
    } else {
        number
    };
    let text = if base == 2 {
        format!("{unsigned:b}")
    } else {
        format!("{unsigned:X}")
    };
    if number < 0 {
        return Ok(text);
    }
    let Some(places) = places else {
        return Ok(text);
    };
    if !places.is_finite() || !(1.0..11.0).contains(&places) {
        return Err(EngineeringError::Num);
    }
    let width = places.trunc() as usize;
    if text.len() > width {
        return Err(EngineeringError::Num);
    }
    Ok(format!("{text:0>width$}"))
}

/// Retained pinned POI complex text spelling.
pub fn complex(
    real: f64,
    imaginary: f64,
    suffix: &str,
    format_number: impl Fn(f64) -> String,
) -> Result<String> {
    // TODO(coercion-non-argument): [LOW] Pinned result spelling intentionally retains historical sign and zero behavior.
    crate::coercion::complex::coerce_complex_pinned_result(real, imaginary, suffix, format_number)
        .map_err(complex_error)
}
/// Retained pinned imaginary coefficient text.
pub fn imaginary(text: &str) -> Result<String> {
    crate::coercion::complex::coercev2_complex_pinned_imaginary(text).map_err(complex_error)
}
/// Retained pinned real coefficient text.
pub fn imreal(text: &str) -> Result<String> {
    crate::coercion::complex::coercev2_complex_pinned_real(text).map_err(complex_error)
}
fn complex_error(error: crate::coercion::CellError) -> EngineeringError {
    if error == crate::coercion::CellError::Value {
        EngineeringError::Value
    } else {
        EngineeringError::Num
    }
}
/// Extract the real component without text round trips.
pub fn complex_real(value: crate::coercion::complex::ComplexNumber) -> f64 {
    value.real
}
/// Extract the imaginary component without text round trips.
pub fn complex_imaginary(value: crate::coercion::complex::ComplexNumber) -> f64 {
    value.imaginary
}
/// Return one for equal numeric operands, otherwise zero.
pub fn delta(a: f64, b: f64) -> f64 {
    if a.to_bits() == b.to_bits() || (a == b && a != 0.0) {
        1.0
    } else {
        0.0
    }
}

/// Commons Math's supported x interval is `[0, 10000]`; order truncates toward zero.
pub fn besselj(x: f64, order: f64) -> Result<f64> {
    if !x.is_finite()
        || !(0.0..=10_000.0).contains(&x)
        || !order.is_finite()
        || !(0.0..i32::MAX as f64).contains(&order)
    {
        return Err(EngineeringError::Num);
    }
    let n = order.trunc() as i32;
    if x == 0.0 {
        return Ok(if n == 0 { 1.0 } else { 0.0 });
    }
    // The absolute power-series bound is (x/2)^n/n! * exp(x²/(4(n+1))).
    // Prove underflow before calling the recurrence, avoiding billions of iterations.
    let nf = n as f64;
    if nf > x && nf * (x / 2.0).ln() - libm::lgamma(nf + 1.0) + x * x / (4.0 * (nf + 1.0)) < -746.0
    {
        return Ok(0.0);
    }
    let result = libm::jn(n, x);
    if result.is_finite() {
        Ok(result)
    } else {
        Err(EngineeringError::Num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_width_signed_base_numbers() {
        assert_eq!(hex2dec("FFFFFFFFFF"), Ok(-1.0));
        assert_eq!(hex2dec("8000000000"), Ok(-549_755_813_888.0));
        assert_eq!(hex2dec("ff"), Ok(255.0));
        assert_eq!(oct2dec("7777777777"), Ok(-1.0));
        assert_eq!(oct2dec("4000000000"), Ok(-536_870_912.0));
        assert_eq!(oct2dec("8"), Err(EngineeringError::Num));
        assert_eq!(hex2dec(""), Ok(0.0));
        assert_eq!(hex2dec("00000000000"), Err(EngineeringError::Num));
        assert_eq!(bin2dec("1111111111"), Ok(-1.0));
        assert_eq!(bin2dec("1000000000"), Ok(-512.0));
        assert_eq!(bin2dec("2"), Ok(2.0));
        assert_eq!(bin2dec("2000000000"), Ok(-512.0));
        assert_eq!(bin2dec("1000000002"), Err(EngineeringError::Num));
    }
    #[test]
    fn binary_java_utf16_digits_and_radix_tail() {
        // Integer.parseInt of each one-char substring: 1*4 + 2*2 + 3.
        assert_eq!(bin2dec("１２３"), Ok(11.0));
        assert_eq!(bin2dec("١٠"), Ok(2.0));
        assert_eq!(bin2dec("１１１１１１１１１１"), Ok(-1.0));
        // Only ASCII zero selects POI's positive ten-character path.
        assert_eq!(bin2dec("０００００００００１"), Ok(-511.0));
        assert_eq!(bin2dec("0１２３４５６７８９"), Ok(1013.0));
        assert_eq!(bin2dec("x+00000001"), Ok(-511.0));
        assert_eq!(bin2dec("𝟙"), Err(EngineeringError::Num));
        assert_eq!(bin2dec("²"), Err(EngineeringError::Num));
        assert_eq!(
            bin2dec("０００００００００００"),
            Err(EngineeringError::Num)
        );
        assert_eq!(hex2dec("１２"), Err(EngineeringError::Num));
    }
    #[test]
    fn decimal_truncation_and_padding() {
        assert_eq!(dec2bin(-512.9, Some(-1.0)), Ok("1000000000".into()));
        assert_eq!(dec2bin(511.9, None), Ok("111111111".into()));
        assert_eq!(dec2bin(-513.0, None), Err(EngineeringError::Num));
        assert_eq!(dec2bin(5.9, Some(4.9)), Ok("0101".into()));
        assert_eq!(dec2bin(5.0, Some(2.0)), Err(EngineeringError::Num));
        assert_eq!(dec2hex(-1.0, Some(f64::NAN)), Ok("FFFFFFFFFF".into()));
        assert_eq!(dec2hex(255.9, Some(4.0)), Ok("00FF".into()));
        assert_eq!(dec2hex(0.0, Some(0.9)), Err(EngineeringError::Num));
    }
    #[test]
    fn permissive_complex_regex_and_poi_spelling() {
        let fmt = |n: f64| n.to_string();
        assert_eq!(complex(0.0, 0.0, "", fmt), Ok("".into()));
        assert_eq!(complex(3.0, -1.0, "i", fmt), Ok("3i".into()));
        assert_eq!(complex(3.0, 2.0, "j", fmt), Ok("3+2j".into()));
        assert_eq!(complex(0.0, 1.0, "I", fmt), Err(EngineeringError::Value));
        assert_eq!(imaginary("3+2i"), Ok("2".into()));
        assert_eq!(imaginary("2i"), Ok("1".into()));
        assert_eq!(imreal("2i"), Ok("2".into()));
        assert_eq!(imaginary("-i"), Ok("1".into()));
        assert_eq!(imaginary("1-2"), Ok("0".into()));
        assert_eq!(imreal(""), Ok("0".into()));
        assert_eq!(imreal("+"), Ok("0".into()));
        assert_eq!(imreal("1e2i"), Err(EngineeringError::Num));
        assert_eq!(imaginary(" 3+2i"), Err(EngineeringError::Num));
    }
    #[test]
    fn delta_java_double_comparison() {
        assert_eq!(delta(0.0, -0.0), 0.0);
        assert_eq!(delta(4.0, 4.0), 1.0);
        assert_eq!(delta(4.0, 4.000000000000001), 0.0);
    }
    #[test]
    fn bessel_recurrence_and_domain() {
        assert!((besselj(1.9, 2.0).unwrap() - 0.329_925_727_692_387).abs() < 1e-14);
        assert!((besselj(1.0, 0.9).unwrap() - 0.765_197_686_557_966_6).abs() < 1e-14);
        assert_eq!(besselj(0.0, 0.0), Ok(1.0));
        assert_eq!(besselj(0.0, 4.0), Ok(0.0));
        assert_eq!(besselj(1.0, 1_000_000.0), Ok(0.0));
        assert_eq!(besselj(-1.0, 2.0), Err(EngineeringError::Num));
        assert_eq!(besselj(10001.0, 0.0), Err(EngineeringError::Num));
        assert_eq!(besselj(1.0, -0.1), Err(EngineeringError::Num));
    }
}
