//! Independently authored signed radix and 48-bit bitwise kernels.
use crate::{EngineeringError::Num, Result};

/// Engineering numeral systems with ten-digit two's-complement notation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Radix {
    /// Ten-bit binary integers.
    Binary,
    /// Thirty-bit octal integers.
    Octal,
    /// Forty-bit hexadecimal integers.
    Hexadecimal,
}
impl Radix {
    fn base(self) -> u32 {
        match self {
            Self::Binary => 2,
            Self::Octal => 8,
            Self::Hexadecimal => 16,
        }
    }
    fn half(self) -> i64 {
        match self {
            Self::Binary => 512,
            Self::Octal => 536_870_912,
            Self::Hexadecimal => 549_755_813_888,
        }
    }
    /// Decode strict ASCII digits at the source's own signed width.
    pub fn decode(self, text: &str) -> Result<i64> {
        if text.len() > 10 {
            return Err(Num);
        }
        let mut value = 0i64;
        for byte in text.bytes() {
            // SAFETY: Each guarded ASCII byte range makes its subtraction nonnegative; the hexadecimal result plus ten is at most fifteen.
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                _ => return Err(Num),
            };
            if digit >= self.base() {
                return Err(Num);
            }
            value = value
                .checked_mul(i64::from(self.base()))
                .and_then(|v| v.checked_add(i64::from(digit)))
                .ok_or(Num)?;
        }
        if text.len() == 10 && value >= self.half() {
            value = value
                .checked_sub(self.half().checked_mul(2).ok_or(Num)?)
                .ok_or(Num)?;
        }
        Ok(value)
    }
    /// Validate the documented decimal domain before truncation, without places.
    pub fn integer(self, number: f64) -> Result<i64> {
        if !number.is_finite()
            || number < -(self.half() as f64)
            || number > self.half() as f64 - 1.0
        {
            return Err(Num);
        }
        Ok(number.trunc() as i64)
    }
    /// Render the destination's signed width, ignoring places for negatives.
    pub fn encode(self, number: i64, places: Option<f64>) -> Result<String> {
        if number < self.half().checked_neg().ok_or(Num)? || number >= self.half() {
            return Err(Num);
        }
        let unsigned = if number < 0 {
            number
                .checked_add(self.half().checked_mul(2).ok_or(Num)?)
                .ok_or(Num)?
        } else {
            number
        };
        let text = match self {
            Self::Binary => format!("{unsigned:b}"),
            Self::Octal => format!("{unsigned:o}"),
            Self::Hexadecimal => format!("{unsigned:X}"),
        };
        if number < 0 {
            return Ok(text);
        }
        let Some(places) = places else {
            return Ok(text);
        };
        let places = places.trunc();
        if !places.is_finite() || !(1.0..=10.0).contains(&places) || text.len() > places as usize {
            return Err(Num);
        }
        let width = places as usize;
        Ok(format!("{text:0>width$}"))
    }
}
/// Constant-work Boolean combination of two 48-bit unsigned integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitOperation {
    /// Intersection of set bits.
    And,
    /// Union of set bits.
    Or,
    /// Exclusive union of set bits.
    Xor,
}
/// Direction of a bit shift before negative-count reversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shift {
    /// Shift toward higher powers of two.
    Left,
    /// Shift toward lower powers of two.
    Right,
}
const MAX: u64 = (1u64 << 48) - 1;
fn operand(number: f64) -> Result<u64> {
    if !number.is_finite() || number.fract() != 0.0 || !(0.0..=MAX as f64).contains(&number) {
        return Err(Num);
    }
    Ok(number as u64)
}
/// Combine admitted integers without allocation or string conversion.
pub fn bitwise(a: f64, b: f64, operation: BitOperation) -> Result<f64> {
    let (a, b) = (operand(a)?, operand(b)?);
    Ok(match operation {
        BitOperation::And => a & b,
        BitOperation::Or => a | b,
        BitOperation::Xor => a ^ b,
    } as f64)
}
/// Shift with a checked count and explicit 48-bit result admission.
pub fn shift(number: f64, count: f64, direction: Shift) -> Result<f64> {
    let number = operand(number)?;
    if !count.is_finite() || count.fract() != 0.0 || !(-53.0..=53.0).contains(&count) {
        return Err(Num);
    }
    let left = (direction == Shift::Left) != (count < 0.0);
    let count = count.abs() as u32;
    let result = if left {
        u128::from(number).checked_shl(count).ok_or(Num)?
    } else {
        u128::from(number).checked_shr(count).ok_or(Num)?
    };
    if result > u128::from(MAX) {
        return Err(Num);
    }
    Ok(result as f64)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_and_destination_have_independent_signed_widths() {
        assert_eq!(Radix::Binary.decode("1111111111"), Ok(-1));
        assert_eq!(Radix::Hexadecimal.encode(-1, None), Ok("FFFFFFFFFF".into()));
        assert_eq!(
            Radix::Octal.encode(-1, Some(f64::NAN)),
            Ok("7777777777".into())
        );
        assert_eq!(Radix::Hexadecimal.decode("FFF"), Ok(4095));
        assert_eq!(Radix::Binary.encode(4095, None), Err(Num));
        for (base, min, max) in [
            (Radix::Binary, -512, 511),
            (Radix::Octal, -536_870_912, 536_870_911),
            (Radix::Hexadecimal, -549_755_813_888, 549_755_813_887),
        ] {
            assert!(base.encode(min, None).is_ok());
            assert!(base.encode(max, None).is_ok());
            assert_eq!(base.encode(min - 1, None), Err(Num));
            assert_eq!(base.encode(max + 1, None), Err(Num));
        }
        assert_eq!(Radix::Hexadecimal.decode("FFFFFFFE00"), Ok(-512));
        assert_eq!(Radix::Octal.encode(8, Some(4.9)), Ok("0010".into()));
        assert_eq!(Radix::Octal.encode(8, Some(1.)), Err(Num));
        for text in ["2", "１２", "+1", "-1", " 1", "00000000000"] {
            assert_eq!(Radix::Binary.decode(text), Err(Num));
        }
        assert_eq!(Radix::Hexadecimal.decode("ff"), Ok(255));
        assert_eq!(Radix::Octal.decode(""), Ok(0));
    }
    #[test]
    fn bit_arithmetic_domains_and_reversed_shifts() {
        assert_eq!(bitwise(10., 12., BitOperation::And), Ok(8.));
        assert_eq!(bitwise(10., 12., BitOperation::Or), Ok(14.));
        assert_eq!(bitwise(10., 12., BitOperation::Xor), Ok(6.));
        assert_eq!(shift(3., 2., Shift::Left), Ok(12.));
        assert_eq!(shift(13., 2., Shift::Right), Ok(3.));
        assert_eq!(shift(13., -2., Shift::Left), Ok(3.));
        assert_eq!(shift(0., 53., Shift::Left), Ok(0.));
        assert_eq!(shift(MAX as f64, 53., Shift::Right), Ok(0.));
        assert_eq!(shift((1u64 << 47) as f64, 1., Shift::Left), Err(Num));
        for n in [-1., 0.5, (1u64 << 48) as f64, f64::NAN] {
            assert_eq!(bitwise(n, 0., BitOperation::Or), Err(Num));
        }
        for n in [-54., 54., 1.5, f64::INFINITY] {
            assert_eq!(shift(1., n, Shift::Left), Err(Num));
        }
    }
}
