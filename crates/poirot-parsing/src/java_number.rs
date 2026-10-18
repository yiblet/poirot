//! Java floating text grammar, including exact hexadecimal rounding.
use num_bigint::BigUint;
const FRACTION: u64 = (1u64 << 52) - 1;
/// Java hexadecimal floating literals, rounded once to IEEE binary64. Decimal
/// and whitespace grammar belongs to the database criterion parser.
pub fn parse_hex(value: &str) -> Option<f64> {
    let (negative, value) = if let Some(rest) = value.strip_prefix('-') {
        (true, rest)
    } else {
        (false, value.strip_prefix('+').unwrap_or(value))
    };
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))?;
    let (mantissa, exponent) = value.split_once(['p', 'P'])?;
    let (exp_negative, digits) = if let Some(rest) = exponent.strip_prefix('-') {
        (true, rest)
    } else {
        (false, exponent.strip_prefix('+').unwrap_or(exponent))
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut exponent = digits.bytes().fold(0i64, |n, c| {
        n.saturating_mul(10).saturating_add((c - b'0') as i64)
    });
    if exp_negative {
        exponent = -exponent;
    }
    let mut fractional_digits = 0i64;
    let mut point = false;
    let mut hex = Vec::new();
    hex.try_reserve_exact(mantissa.len()).ok()?;
    for character in mantissa.bytes() {
        if character == b'.' && !point {
            point = true;
            continue;
        }
        if !character.is_ascii_hexdigit() {
            return None;
        }
        hex.push(character);
        if point {
            fractional_digits += 1;
        }
    }
    let mut integer = BigUint::parse_bytes(&hex, 16)?;
    let bits = integer.bits() as i64;
    let sign = if negative { 1u64 << 63 } else { 0 };
    if bits == 0 {
        return Some(f64::from_bits(sign));
    }
    let scale = exponent.saturating_sub(fractional_digits.saturating_mul(4));
    let leading = scale.saturating_add(bits - 1);
    if leading > 1024 {
        return Some(f64::from_bits(sign | (2047u64 << 52)));
    }
    if leading < -1075 {
        return Some(f64::from_bits(sign));
    }
    let discard = (bits - 53).max(-1074 - scale).max(0) as usize;
    if discard > 0 {
        let quotient = &integer >> discard;
        let remainder = &integer - (&quotient << discard);
        let half = BigUint::from(1u8) << (discard - 1);
        let round = remainder > half || (remainder == half && quotient.bit(0));
        integer = quotient + BigUint::from(u8::from(round));
    }
    let scale = scale + discard as i64;
    let rounded_bits = integer.bits() as i64;
    if rounded_bits == 0 {
        return Some(f64::from_bits(sign));
    }
    let leading = scale + rounded_bits - 1;
    if leading > 1023 {
        return Some(f64::from_bits(sign | (2047u64 << 52)));
    }
    let words = integer.to_u64_digits();
    let integer = *words.first()?;
    let raw = if leading >= -1022 {
        // Carry after rounding may have produced a 54-bit significand.
        let normalized = if rounded_bits > 53 {
            integer >> (rounded_bits - 53)
        } else {
            integer << (53 - rounded_bits)
        };
        (((leading + 1023) as u64) << 52) | (normalized & FRACTION)
    } else {
        integer << (scale + 1074)
    };
    Some(f64::from_bits(sign | raw))
}

/// Java's decimal floating syntax, including type suffixes and ASCII trim.
pub fn parse_java_number(value: &str) -> Option<f64> {
    let value = value.trim_matches(|c: char| c <= '\u{20}');
    match value {
        "NaN" | "+NaN" | "-NaN" => return Some(f64::NAN),
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let value = value.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(value);
    let unsigned = value.trim_start_matches(['+', '-']);
    if unsigned.starts_with("0x") || unsigned.starts_with("0X") {
        return parse_hex(value);
    }
    if value
        .bytes()
        .any(|c| !matches!(c, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return None;
    }
    value.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hexadecimal_binary64_rounding_boundaries() {
        assert_eq!(parse_hex("0x1.8p1"), Some(3.));
        assert_eq!(parse_hex("0x1.00000000000008p0"), Some(1.));
        assert_eq!(
            parse_hex("0x1.00000000000009p0"),
            Some(f64::from_bits(1f64.to_bits() + 1))
        );
        assert_eq!(
            parse_hex("0x0.0000000000001p-1022"),
            Some(f64::from_bits(1))
        );
        assert_eq!(parse_hex("0x1p-1075"), Some(0.));
        assert_eq!(
            parse_hex("0x1.0000000000001p-1075"),
            Some(f64::from_bits(1))
        );
        assert_eq!(parse_hex("-0x1p1024"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_hex("0x1p-999999999999999999999999"), Some(0.));
        assert_eq!(parse_hex("0x1p0p1"), None);
        assert_eq!(parse_hex("0xp0"), None);
    }
}
