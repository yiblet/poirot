//! Checked unsigned positional digit codec, independent of spreadsheet grammars.
use crate::scalar::CellError;

/// A radix with a proven ASCII alphabet in the interval 2 through 36.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Radix(u8);
impl Radix {
    /// Validate the base before division, remainder, or digit admission.
    pub fn new(value: u8) -> Result<Self, CellError> {
        if (2..=36).contains(&value) {
            Ok(Self(value))
        } else {
            Err(CellError::Num)
        }
    }
    /// Positional multiplier.
    pub fn value(self) -> u8 {
        self.0
    }
    /// Case-insensitive ASCII digit admission under this radix.
    pub fn digit(self, byte: u8) -> Result<u8, CellError> {
        // SAFETY: Each branch bounds byte to its ASCII interval before subtraction; letter offsets are at most 25 and adding 10 is at most 35.
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'A'..=b'Z' => byte - b'A' + 10,
            b'a'..=b'z' => byte - b'a' + 10,
            _ => return Err(CellError::Num),
        };
        if digit < self.0 {
            Ok(digit)
        } else {
            Err(CellError::Num)
        }
    }
    /// Canonical uppercase spelling of one admitted digit.
    pub fn symbol(self, digit: u8) -> Result<char, CellError> {
        if !(2..=36).contains(&self.0) || digit >= self.0 || digit > 35 {
            return Err(CellError::Num);
        }
        // SAFETY: The local check bounds digit to 0..=35; the decimal branch adds at most 9 to 48, and the alphabetic branch adds at most 35 to 65 before subtracting 10 with digit >= 10.
        Ok(char::from(if digit < 10 {
            b'0' + digit
        } else {
            b'A' + digit - 10
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radix_alphabet_is_checked_and_case_insensitive() {
        for value in 2..=36 {
            let radix = Radix::new(value).unwrap();
            for digit in 0..value {
                let symbol = radix.symbol(digit).unwrap();
                assert_eq!(radix.digit(symbol as u8), Ok(digit));
                assert_eq!(radix.digit(symbol.to_ascii_lowercase() as u8), Ok(digit));
            }
            assert_eq!(radix.symbol(value), Err(CellError::Num));
        }
        for value in [0, 1, 37, 255] {
            assert_eq!(Radix::new(value), Err(CellError::Num));
        }
    }
}
