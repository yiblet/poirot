//! Unsigned positional result rendering, independent of argument coercion.
use crate::numeric::radix::Radix;
use crate::scalar::CellError;

/// Exact bounded output size, available before the host admits allocation.
pub fn base_length(mut number: u64, radix: Radix, minimum: usize) -> Result<usize, CellError> {
    if number >= 9007199254740992 || minimum > 255 {
        return Err(CellError::Num);
    }
    let multiplier = u64::from(radix.value());
    if !(2..=36).contains(&multiplier) {
        return Err(CellError::Num);
    }
    let mut digits: usize = 1;
    while number >= multiplier {
        number = number.checked_div(multiplier).ok_or(CellError::Num)?;
        digits = digits.saturating_add(1);
    }
    Ok(digits.max(minimum))
}

/// Render an unsigned integer with zero padding. Length is checked locally;
/// hosts must admit the bounded output allocation/work before calling.
pub fn base(mut number: u64, radix: Radix, minimum: usize) -> Result<String, CellError> {
    let multiplier = u64::from(radix.value());
    if !(2..=36).contains(&multiplier) {
        return Err(CellError::Num);
    }
    let length = base_length(number, radix, minimum)?;
    if length > 255 || minimum > 255 {
        return Err(CellError::Num);
    }
    // SAFETY: The local check bounds length to 255 char elements, well below isize::MAX bytes.
    let mut reversed = Vec::with_capacity(length);
    loop {
        // SAFETY: multiplier is locally checked >=2; division decreases any positive u64 each iteration, so at most 64 digits are pushed, well below the isize::MAX allocation limit.
        reversed.push(radix.symbol((number.checked_rem(multiplier).ok_or(CellError::Num)?) as u8)?);
        number = number.checked_div(multiplier).ok_or(CellError::Num)?;
        if number == 0 {
            break;
        }
    }
    if reversed.len() > 255 {
        return Err(CellError::Num);
    }
    // SAFETY: Both the locally checked vector length and minimum are at most 255, so resizing cannot request more than 255 char elements.
    reversed.resize(reversed.len().max(minimum), '0');
    // TODO(coercion-non-argument): [LOW] BASE intentionally renders an unsigned numeric result as canonical uppercase positional text with leading zero padding.
    // SAFETY: The locally checked vector has at most 255 chars, so collecting their UTF-8 encoding needs at most 1020 bytes.
    Ok(reversed.into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radix_conversions_share_checked_digits_and_explicit_domains() {
        // Microsoft documented examples, plus independent positional arithmetic.
        for (n, r, p, text) in [
            (255, 2, 3, "11111111"),
            (100, 16, 0, "64"),
            (15, 2, 10, "0000001111"),
            (45745, 36, 0, "ZAP"),
            (0, 10, 0, "0"),
        ] {
            assert_eq!(base(n, Radix::new(r).unwrap(), p).unwrap(), text);
        }
        for radix in [2, 10, 16, 36] {
            let radix = Radix::new(radix).unwrap();
            for number in [0, 1, 15, 255, 45745, 9007199254740991] {
                let text = base(number, radix, 0).unwrap();
                assert_eq!(
                    crate::coercion::radix::coercev2_decimal_text(&text, radix),
                    Ok(number as f64)
                );
            }
        }
        assert_eq!(
            base(9007199254740992, Radix::new(2).unwrap(), 0),
            Err(CellError::Num)
        );
        assert_eq!(base(0, Radix::new(2).unwrap(), 256), Err(CellError::Num));
    }
}
