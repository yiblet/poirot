//! Fixed-width execution of the existing positive decimal scaling path.
//! The parent module credits the Apache POI arithmetic reproduced here.

use super::FRACTION;

/// Exact positive powers within the bounded normalization path.
static POWERS_OF_FIVE: [u128; 25] = {
    let mut powers = [1u128; 25];
    let mut index = 1;
    while index < powers.len() {
        // SAFETY: This loop bounds both indexes and every power is below 5^25.
        powers[index] = powers[index - 1] * 5;
        index += 1;
    }
    powers
};

/// Return normalized binary parts when both decimal scaling steps fit in 128 bits.
pub(super) fn normalize(bits: u64, exponent: i32, mut power: i32) -> Option<(i32, i32, u64)> {
    // The implicit bit and the 52 fraction bits occupy 53 bits.
    let initial = u128::from((bits & FRACTION) | (1 << 52)) << 11;
    let first_scale = power.checked_neg()?;
    let (mut significand, mut exponent) = scale(initial, exponent, first_scale)?;
    let bits = width(significand).checked_sub(64)?;
    let normalized = exponent.checked_add(bits as i32)?;
    match normalized {
        46 if significand > u128::from(0xb5e6_20f4_7fff_e666u64).checked_shl(bits)? => {}
        44..=46 => {
            (significand, exponent) = scale(significand, exponent, 1)?;
            power = power.checked_sub(1)?;
        }
        47 | 48 => {}
        49 if significand < u128::from(0xe35f_a931_9fff_e000u64).checked_shl(bits)? => {}
        // Negative scaling uses the general reciprocal path.
        _ => return None,
    }
    let (exponent, significand) = round(exponent, significand)?;
    Some((power, exponent, significand))
}

/// Multiply by the exact small power, discarding the same complete limbs as the general path.
fn scale(significand: u128, exponent: i32, power: i32) -> Option<(u128, i32)> {
    let index = usize::try_from(power).ok()?;
    let product = significand.checked_mul(*POWERS_OF_FIVE.get(index)?)?;
    let bits = width(product) as i32;
    // SAFETY: A u128 has at most 128 bits, so this expression fits in i32.
    let discard = (((bits - 72) / 32) * 32).max(0);
    let exponent = exponent.checked_add(power)?.checked_add(discard)?;
    Some((product.checked_shr(discard as u32)?, exponent))
}

/// Round the remaining high bits at the same half-up threshold as the general path.
fn round(exponent: i32, significand: u128) -> Option<(i32, u64)> {
    let bits = width(significand);
    let discard = bits.checked_sub(64)?;
    let exponent = exponent.checked_add(discard as i32)?;
    let Some(adjusted) = discard.checked_sub(1) else {
        return Some((exponent, u64::try_from(significand).ok()?));
    };
    // SAFETY: adjusted is derived from a u128 bit width and is below 64.
    let whole_limbs = (adjusted / 32) * 32;
    let high = significand.checked_shr(whole_limbs)?;
    let remaining = discard.checked_sub(whole_limbs)?;
    let half = 1u128.checked_shl(remaining.checked_sub(1)?)?;
    let rounded = high.checked_add(half)?;
    let carry = i32::from(width(rounded) > width(high));
    let exponent = exponent.checked_add(carry)?;
    let shift = remaining.checked_add(carry as u32)?;
    Some((exponent, u64::try_from(rounded.checked_shr(shift)?).ok()?))
}

/// Count significant bits of a machine integer.
fn width(value: u128) -> u32 {
    // SAFETY: u128::leading_zeros() returns at most 128.
    128 - value.leading_zeros()
}
