//! Excel's 15 digit comparison, derived from POI NumberComparer,
//! NormalisedDecimal and MutableFPNumber (pinned source inventory).
use num_bigint::BigUint;
use poirot_functions_core::CellError;
use std::cmp::Ordering;
const FRACTION: u64 = (1u64 << 52) - 1;
/// Source-pinned numeric-to-text conversion independent of workbook formatting.
pub mod number_text;

/// Compare finite numbers with the pinned Excel-style fifteen-digit normalization.
pub fn compare(a: f64, b: f64) -> Result<Ordering, CellError> {
    if !a.is_finite() || !b.is_finite() {
        return Err(CellError::Num);
    }
    let (ar, br) = (a.to_bits(), b.to_bits());
    let (ae, be) = (((ar >> 52) & 2047) as i32, ((br >> 52) & 2047) as i32);
    let negative = a.is_sign_negative();
    if negative != b.is_sign_negative() {
        return Ok(if negative {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let reverse = |c: Ordering| if negative { c.reverse() } else { c };
    if (ae - be).abs() > 1 {
        return Ok(reverse(ae.cmp(&be)));
    }
    if ar == br {
        return Ok(Ordering::Equal);
    }
    if ae == 0 && be == 0 {
        return Ok(reverse((ar & FRACTION).cmp(&(br & FRACTION))));
    }
    if ae == 0 {
        return Ok(across_threshold(br, ar, negative).reverse());
    }
    if be == 0 {
        return Ok(across_threshold(ar, br, negative));
    }
    Ok(reverse(
        decimal(ar, ae - 1023)?.cmp(&decimal(br, be - 1023)?),
    ))
}
fn across_threshold(normal: u64, subnormal: u64, negative: bool) -> Ordering {
    let fraction = subnormal & FRACTION;
    let anomalous = fraction != 0 && (normal & FRACTION) <= 7 && fraction >= 0x000f_ffff_ffff_fffa;
    if anomalous && (normal & FRACTION) == 7 && fraction == 0x000f_ffff_ffff_fffa {
        return Ordering::Equal;
    }
    let c = if anomalous {
        Ordering::Less
    } else {
        Ordering::Greater
    };
    if negative { c.reverse() } else { c }
}
/// Mutable high-precision significand/exponent pair used during decimal normalization.
struct Binary {
    significand: BigUint,
    exponent: i32,
}
impl Binary {
    fn mul_ten(&mut self, power: i32) {
        let index = power.unsigned_abs();
        let five = BigUint::from(5u8).pow(index);
        let bits = five.bits() as i32;
        let (multiplier, shift) = if power < 0 {
            // SAFETY: five is locally computed as 5 raised to an unsigned power and is therefore nonzero.
            let reciprocal = (BigUint::from(1u8) << (80 + bits) as usize) / five;
            let adjustment = reciprocal.bits() as i32 - 80;
            (
                reciprocal >> adjustment as usize,
                -(bits - adjustment + index as i32 + 80),
            )
        } else if bits > 68 {
            (five >> (bits - 68) as usize, index as i32 + bits - 68)
        } else {
            (five, index as i32)
        };
        self.significand *= multiplier;
        self.exponent += shift;
        let discard = ((self.significand.bits() as i32 - 72) / 32) * 32;
        if discard > 0 {
            self.significand >>= discard as usize;
            self.exponent += discard;
        }
    }
    fn normalized_exponent(&self) -> i32 {
        self.exponent + self.significand.bits() as i32 - 64
    }
    fn threshold(&self, threshold: u64) -> Ordering {
        self.significand
            .cmp(&(BigUint::from(threshold) << (self.significand.bits() as usize - 64)))
    }
    fn normalize(&mut self) {
        let mut old_bits = self.significand.bits() as i32;
        let mut discard = old_bits - 64;
        if discard == 0 {
            return;
        }
        self.exponent += discard;
        if discard > 32 {
            let high = ((discard - 1) / 32) * 32;
            self.significand >>= high as usize;
            discard -= high;
            old_bits -= high;
        }
        self.significand += BigUint::from(1u8) << (discard - 1) as usize;
        if self.significand.bits() as i32 > old_bits {
            discard += 1;
            self.exponent += 1;
        }
        self.significand >>= discard as usize;
    }
}
fn decimal(bits: u64, exponent: i32) -> Result<(i32, u64), CellError> {
    if !(-1022..=1024).contains(&exponent) {
        return Err(CellError::Num);
    }
    let mut power = if !(46..=49).contains(&exponent) {
        -(((29 << 19) - exponent * 315653 + (1 << 19)) >> 20)
    } else {
        0
    };
    let mut binary = Binary {
        significand: BigUint::from((bits & FRACTION) | (1 << 52)) << 11usize,
        exponent,
    };
    if power != 0 {
        binary.mul_ten(-power);
    }
    match binary.normalized_exponent() {
        46 if binary.threshold(0xb5e6_20f4_7fff_e666).is_gt() => {}
        44..=46 => {
            binary.mul_ten(1);
            power -= 1;
        }
        47 | 48 => {}
        49 if binary.threshold(0xe35f_a931_9fff_e000).is_lt() => {}
        49 | 50 => {
            binary.mul_ten(-1);
            power += 1;
        }
        _ => return Err(CellError::Num),
    }
    binary.normalize();
    let words = binary.significand.to_u64_digits();
    let significand = *words.first().ok_or(CellError::Num)?;
    if !(0..=63).contains(&binary.exponent) {
        return Err(CellError::Num);
    }
    let fraction = ((significand as u32).wrapping_shl((binary.exponent - 39) as u32)) & 0x00ff_ff80;
    let mut whole = significand >> (63 - binary.exponent) as usize;
    if fraction >= 0x0080_0000 {
        whole += 1;
    }
    if whole >= 1_000_000_000_000_000 {
        whole /= 10;
        power += 1;
    }
    Ok((power, whole))
}

/// POI NumberToTextConverter representation, including the 20-character
/// scientific-notation boundary and Excel's zero display for subnormals.
pub fn coerce_number_to_text(value: f64) -> String {
    // POI toText uses Double.doubleToLongBits, which canonicalizes every NaN.
    let raw = if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    };
    coerce_number_bits_to_text(raw)
}

/// POI's package-private rawDoubleBitsToText, distinct from its public toText.
pub fn coerce_number_bits_to_text(raw: u64) -> String {
    let negative = raw >> 63 != 0;
    let exponent = ((raw >> 52) & 2047) as i32;
    let sign = if negative && exponent != 2047 {
        "-"
    } else {
        ""
    };
    if exponent == 0 {
        return format!("{sign}0");
    }
    let Ok((power, whole)) = decimal(raw, exponent - 1023) else {
        return CellError::Num.to_string();
    };
    if whole == 0 {
        return format!("{sign}0");
    }
    if !(-400..=400).contains(&power) {
        return CellError::Num.to_string();
    }
    let mut exp = power + 14;
    let digits = if exp.abs() > 98 {
        let Some(rounded_whole) = whole.checked_add(5) else {
            return CellError::Num.to_string();
        };
        let mut rounded = rounded_whole.to_string();
        let last = rounded.len() - 1;
        // SAFETY: rounded is a nonempty decimal integer string; len() - 1 starts its final ASCII digit.
        rounded.replace_range(last.., "0");
        if rounded.len() == 16 {
            exp += 1;
        }
        rounded
    } else {
        whole.to_string()
    };
    let significant = digits.trim_end_matches('0');
    let scientific = if exp < 0 {
        2 + (-exp - 1) as usize + significant.len() > 20
    } else {
        exp > 19
    };
    if scientific {
        let fraction = if significant.len() > 1 {
            // SAFETY: whole is checked nonzero here, so its decimal digits and trimmed significant prefix are nonempty ASCII; all slice endpoints use their measured lengths.
            format!(".{}", &significant[1..])
        } else {
            String::new()
        };
        return format!(
            "{sign}{}{fraction}E{}{:02}",
            // SAFETY: whole is checked nonzero here, so its decimal digits and trimmed significant prefix are nonempty ASCII; all slice endpoints use their measured lengths.
            &significant[..1],
            if exp < 0 { "-" } else { "+" },
            exp.abs()
        );
    }
    if exp < 0 {
        // SAFETY: The local power check bounds exp to -386..=415; this negative-exp branch repeats a single ASCII byte at most 385 times.
        return format!("{sign}0.{}{}", "0".repeat((-exp - 1) as usize), significant);
    }
    let decimal_position = exp as usize + 1;
    if decimal_position < significant.len() {
        format!(
            "{sign}{}.{}",
            // SAFETY: whole is checked nonzero here, so its decimal digits and trimmed significant prefix are nonempty ASCII; all slice endpoints use their measured lengths.
            &significant[..decimal_position],
            // SAFETY: whole is checked nonzero here, so its decimal digits and trimmed significant prefix are nonempty ASCII; all slice endpoints use their measured lengths.
            &significant[decimal_position..]
        )
    } else {
        format!(
            "{sign}{significant}{}",
            // SAFETY: This branch requires decimal_position >= significant.len(); the local power bound keeps the single-byte repeat below 418 bytes.
            "0".repeat(decimal_position - significant.len())
        )
    }
}

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
    let mut hex = Vec::with_capacity(mantissa.len());
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
    fn pinned_number_comparison_step_transitions() {
        for (a, b, expected) in [
            (0x4010000000000004u64, 0x4010000000000005u64, 0i32),
            (0x4010000000000005u64, 0x4010000000000006u64, -1i32),
            (0x4010000000000006u64, 0x4010000000000007u64, 0i32),
            (0x401000000000000fu64, 0x4010000000000010u64, 0i32),
            (0x4010000000000010u64, 0x4010000000000011u64, -1i32),
            (0x4010000000000011u64, 0x4010000000000012u64, 0i32),
            (0x401000000000001bu64, 0x401000000000001cu64, 0i32),
            (0x401000000000001cu64, 0x401000000000001du64, -1i32),
            (0x401000000000001du64, 0x401000000000001eu64, 0i32),
            (0x403ce0fffffffff0u64, 0x403ce0fffffffff1u64, 0i32),
            (0x403ce0fffffffff1u64, 0x403ce0fffffffff2u64, -1i32),
            (0x403ce0fffffffff2u64, 0x403ce0fffffffff3u64, 0i32),
            (0x5010000000000005u64, 0x5010000000000006u64, 0i32),
            (0x5010000000000006u64, 0x5010000000000007u64, -1i32),
            (0x5010000000000007u64, 0x5010000000000008u64, 0i32),
            (0x501000000000000fu64, 0x5010000000000010u64, 0i32),
            (0x5010000000000010u64, 0x5010000000000011u64, -1i32),
            (0x5010000000000011u64, 0x5010000000000012u64, 0i32),
            (0x5010000000000019u64, 0x501000000000001au64, 0i32),
            (0x501000000000001au64, 0x501000000000001bu64, -1i32),
            (0x501000000000001bu64, 0x501000000000001cu64, 0i32),
            (0x544ce6345cf32017u64, 0x544ce6345cf32018u64, 0i32),
            (0x544ce6345cf32018u64, 0x544ce6345cf32019u64, -1i32),
            (0x544ce6345cf32019u64, 0x544ce6345cf3201au64, 0i32),
            (0x544ce6345cf32059u64, 0x544ce6345cf3205au64, 0i32),
            (0x544ce6345cf3205au64, 0x544ce6345cf3205bu64, -1i32),
            (0x544ce6345cf3205bu64, 0x544ce6345cf3205cu64, 0i32),
            (0x544ce6345cf3209bu64, 0x544ce6345cf3209cu64, 0i32),
            (0x544ce6345cf3209cu64, 0x544ce6345cf3209du64, -1i32),
            (0x544ce6345cf3209du64, 0x544ce6345cf3209eu64, 0i32),
            (0x544ce6345cf320ddu64, 0x544ce6345cf320deu64, 0i32),
            (0x544ce6345cf320deu64, 0x544ce6345cf320dfu64, -1i32),
            (0x544ce6345cf320dfu64, 0x544ce6345cf320e0u64, 0i32),
            (0x54b250001000101cu64, 0x54b250001000101du64, 0i32),
            (0x54b250001000101du64, 0x54b250001000101eu64, -1i32),
            (0x54b250001000101eu64, 0x54b250001000101fu64, 0i32),
            (0x54b250001000104fu64, 0x54b2500010001050u64, 0i32),
            (0x54b2500010001050u64, 0x54b2500010001051u64, -1i32),
            (0x54b2500010001051u64, 0x54b2500010001052u64, 0i32),
            (0x54b2500010001082u64, 0x54b2500010001083u64, 0i32),
            (0x54b2500010001083u64, 0x54b2500010001084u64, -1i32),
            (0x54b2500010001084u64, 0x54b2500010001085u64, 0i32),
            (0x6230100010000fffu64, 0x6230100010001000u64, 0i32),
            (0x6230100010001000u64, 0x6230100010001001u64, -1i32),
            (0x6230100010001001u64, 0x6230100010001002u64, 0i32),
            (0x6230100010001004u64, 0x6230100010001005u64, 0i32),
            (0x6230100010001005u64, 0x6230100010001006u64, -1i32),
            (0x6230100010001006u64, 0x6230100010001007u64, 0i32),
            (0x6230100010001009u64, 0x623010001000100au64, 0i32),
            (0x623010001000100au64, 0x623010001000100bu64, -1i32),
            (0x623010001000100bu64, 0x623010001000100cu64, 0i32),
            (0x7f50300020001010u64, 0x7f50300020001011u64, 0i32),
            (0x7f50300020001011u64, 0x7f50300020001012u64, -1i32),
            (0x7f50300020001012u64, 0x7f50300020001013u64, 0i32),
            (0x7f5030002000102au64, 0x7f5030002000102bu64, 0i32),
            (0x7f5030002000102bu64, 0x7f5030002000102cu64, -1i32),
            (0x7f5030002000102cu64, 0x7f5030002000102du64, 0i32),
            (0x7f50300020001043u64, 0x7f50300020001044u64, 0i32),
            (0x7f50300020001044u64, 0x7f50300020001045u64, -1i32),
            (0x7f50300020001045u64, 0x7f50300020001046u64, 0i32),
            (0x2b2bffff10001029u64, 0x2b2bffff1000102au64, 0i32),
            (0x2b2bffff1000102au64, 0x2b2bffff1000102bu64, -1i32),
            (0x2b2bffff1000102bu64, 0x2b2bffff1000102cu64, 0i32),
            (0x2b2bffff10001078u64, 0x2b2bffff10001079u64, 0i32),
            (0x2b2bffff10001079u64, 0x2b2bffff1000107au64, -1i32),
            (0x2b2bffff1000107au64, 0x2b2bffff1000107bu64, 0i32),
            (0x2b2bffff100010c7u64, 0x2b2bffff100010c8u64, 0i32),
            (0x2b2bffff100010c8u64, 0x2b2bffff100010c9u64, -1i32),
            (0x2b2bffff100010c9u64, 0x2b2bffff100010cau64, 0i32),
            (0x2b2bff001000102cu64, 0x2b2bff001000102du64, 0i32),
            (0x2b2bff001000102du64, 0x2b2bff001000102eu64, -1i32),
            (0x2b2bff001000102eu64, 0x2b2bff001000102fu64, 0i32),
            (0x2b2bff0010001034u64, 0x2b2bff0010001035u64, 0i32),
            (0x2b2bff0010001035u64, 0x2b2bff0010001036u64, -1i32),
            (0x2b2bff0010001036u64, 0x2b2bff0010001037u64, 0i32),
            (0x2b2bff001000103cu64, 0x2b2bff001000103du64, 0i32),
            (0x2b2bff001000103du64, 0x2b2bff001000103eu64, -1i32),
            (0x2b2bff001000103eu64, 0x2b2bff001000103fu64, 0i32),
            (0x2b61800040002023u64, 0x2b61800040002024u64, 0i32),
            (0x2b61800040002024u64, 0x2b61800040002025u64, -1i32),
            (0x2b61800040002025u64, 0x2b61800040002026u64, 0i32),
            (0x2b61800040002054u64, 0x2b61800040002055u64, 0i32),
            (0x2b61800040002055u64, 0x2b61800040002056u64, -1i32),
            (0x2b61800040002056u64, 0x2b61800040002057u64, 0i32),
            (0x2b61800040002085u64, 0x2b61800040002086u64, 0i32),
            (0x2b61800040002086u64, 0x2b61800040002087u64, -1i32),
            (0x2b61800040002087u64, 0x2b61800040002088u64, 0i32),
            (0x008000000000000au64, 0x008000000000000bu64, 0i32),
            (0x008000000000000bu64, 0x008000000000000cu64, -1i32),
            (0x008000000000000cu64, 0x008000000000000du64, 0i32),
            (0x0010000000000006u64, 0x0010000000000007u64, 0i32),
            (0x0010000000000007u64, 0x0010000000000008u64, -1i32),
            (0x0010000000000008u64, 0x0010000000000009u64, 0i32),
            (0x001000000000001au64, 0x001000000000001bu64, 0i32),
            (0x001000000000001bu64, 0x001000000000001cu64, -1i32),
            (0x001000000000001cu64, 0x001000000000001du64, 0i32),
            (0x001000000000002eu64, 0x001000000000002fu64, 0i32),
            (0x001000000000002fu64, 0x0010000000000030u64, -1i32),
            (0x0010000000000030u64, 0x0010000000000031u64, 0i32),
            (0xbf30000000000000u64, 0xbe60000000000000u64, -1i32),
            (0x0000000000000000u64, 0x8000000000000000u64, 1i32),
            (0x0000000000000000u64, 0x0000000000000001u64, -1i32),
            (0x0008000000000000u64, 0x0008000000000001u64, -1i32),
            (0x000fffffffffffffu64, 0x000ffffffffffffeu64, 1i32),
            (0x000ffffffffffffbu64, 0x000ffffffffffffcu64, -1i32),
            (0x000ffffffffffffbu64, 0x000ffffffffffffeu64, -1i32),
            (0x000fffffffffffffu64, 0x0010000000000000u64, 1i32),
            (0x000ffffffffffffbu64, 0x0010000000000007u64, 1i32),
            (0x000ffffffffffffau64, 0x0010000000000007u64, 0i32),
            (0x000ffffffffffff9u64, 0x0010000000000007u64, -1i32),
            (0x000ffffffffffffau64, 0x0010000000000008u64, -1i32),
            (0x000ffffffffffffbu64, 0x0010000000000008u64, -1i32),
        ] {
            let expected = expected.cmp(&0);
            let a = f64::from_bits(a);
            let b = f64::from_bits(b);
            assert_eq!(compare(a, b), Ok(expected));
            assert_eq!(compare(b, a), Ok(expected.reverse()));
            assert_eq!(compare(-a, -b), Ok(expected.reverse()));
            assert_eq!(compare(-b, -a), Ok(expected));
        }
    }
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
    #[test]
    fn pinned_number_to_text_documented_examples() {
        for (bits, expected) in [
            (0x0000000000000000u64, "0"),
            (0x3FF0000000000000u64, "1"),
            (0x3FF00068DB8BAC71u64, "1.0001"),
            (0x4087A00000000000u64, "756"),
            (0x401E3D70A3D70A3Du64, "7.56"),
            (0x405EDD3C07FB4C99u64, "123.456789012346"),
            (0x4132D687E3DF2180u64, "1234567.89012346"),
            (0x3EE9E409302678BAu64, "1.23456789012346E-05"),
            (0x3F202E85BE180B74u64, "0.000123456789012346"),
            (0x3F543A272D9E0E51u64, "0.00123456789012346"),
            (0x3F8948B0F90591E6u64, "0.0123456789012346"),
            (0x3EE9E409301B5A02u64, "0.0000123456789"),
            (0x3E6E7D05BDABDE50u64, "0.000000056789012345"),
            (0x3E6E7D05BDAD407Eu64, "5.67890123456E-08"),
            (0x3E6E7D06029F18BEu64, "0.00000005678902"),
            (0x2BCB5733CB32AE6Eu64, "9.99999999999912E-98"),
            (0x2B617F7D4ED8C59Eu64, "1.0000000000001E-99"),
            (0x0036319916D67853u64, "1.2345678901235E-307"),
            (0x359DEE7A4AD4B81Fu64, "2E-50"),
            (0x41678C29DCD6E9E0u64, "12345678.9012346"),
            (0x42A674E79C5FE523u64, "12345678901234.6"),
            (0x42DC12218377DE6Bu64, "123456789012346"),
            (0x43118B54F22AEB03u64, "1234567890123460"),
            (0x43E56A95319D63E1u64, "12345678901234600000"),
            (0x441AC53A7E04BCDAu64, "1.23456789012346E+20"),
            (0xC3E56A95319D63E1u64, "-12345678901234600000"),
            (0xC41AC53A7E04BCDAu64, "-1.23456789012346E+20"),
            (0x54820FE0BA17F46Du64, "1.2345678901235E+99"),
            (0x54B693D8E89DF188u64, "1.2345678901235E+100"),
            (0x4A611B0EC57E649Au64, "2E+50"),
            (0x7FEFFFFFFFFFFFFFu64, "1.7976931348623E+308"),
            (0x0010000000000000u64, "2.2250738585072E-308"),
            (0x000FFFFFFFFFFFFFu64, "0"),
            (0x0000000000000001u64, "0"),
            (0x441AC7A08EAD02F2u64, "1.235E+20"),
            (0x40FE26BFFFFFFFF9u64, "123500"),
            (0x3E4A857BFB2F2809u64, "0.00000001235"),
            (0x3BCD291DEF868C89u64, "1.235E-20"),
            (0x444B1AE4D6E2EF4Fu64, "1E+21"),
            (0x412E847FFFFFFFFFu64, "1000000"),
            (0x3E45798EE2308C39u64, "0.00000001"),
            (0x3C32725DD1D243ABu64, "0.000000000000000001"),
            (0x3BFD83C94FB6D2ABu64, "1E-19"),
            (0xC44B1AE4D6E2EF4Fu64, "-1E+21"),
            (0xC12E847FFFFFFFFFu64, "-1000000"),
            (0xBE45798EE2308C39u64, "-0.00000001"),
            (0xBC32725DD1D243ABu64, "-0.000000000000000001"),
            (0xBBFD83C94FB6D2ABu64, "-1E-19"),
        ] {
            assert_eq!(
                coerce_number_to_text(f64::from_bits(bits)),
                expected,
                "bits={bits:016x}"
            );
        }
    }
    #[test]
    fn rounded_decimal_signed_zero_and_threshold() {
        assert_eq!(compare(0.1 + 0.2, 0.3), Ok(Ordering::Equal));
        assert_eq!(compare(1.000000000000001, 1.), Ok(Ordering::Equal));
        assert_eq!(compare(1.00000000000001, 1.), Ok(Ordering::Greater));
        assert_eq!(compare(-0., 0.), Ok(Ordering::Less));
        assert_eq!(
            compare(
                f64::from_bits((1 << 52) | 7),
                f64::from_bits(0x000f_ffff_ffff_fffa)
            ),
            Ok(Ordering::Equal)
        );
        assert_eq!(
            compare(f64::MIN_POSITIVE, f64::from_bits(0x000f_ffff_ffff_fffb)),
            Ok(Ordering::Less)
        );
        assert_eq!(compare(1e300, 1e-300), Ok(Ordering::Greater));
    }
}
