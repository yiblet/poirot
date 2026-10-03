//! Allocation-free plans for lossless POI text operations.
//! Host checks Reservations before copying the selected/output units.
use std::ops::Range;

/// Selection/reservation failure before copying UTF-16 text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanFailure {
    /// Invalid text-function position or count.
    Value,
    /// Pinned Java index calculation overflowed.
    JavaIndexOverflow,
    /// Output-size or reservation arithmetic overflowed.
    SizeOverflow,
}
/// Text bytes, retained elements, and work to admit before executing a UTF-16 plan.
/// Text storage counts two bytes per code unit, not Unicode characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reservation {
    /// Two bytes per retained UTF-16 code unit.
    pub text_bytes: usize,
    /// Logical retained code-unit count.
    pub elements: usize,
    /// Work units the host must admit before execution.
    pub work: usize,
}
impl Reservation {
    /// Build a checked UTF-16 byte/element reservation.
    pub fn units(units: usize, work: usize) -> Result<Self, PlanFailure> {
        Ok(Self {
            text_bytes: units.checked_mul(2).ok_or(PlanFailure::SizeOverflow)?,
            elements: units,
            work,
        })
    }
}
/// Plan a clamped prefix range without allocating output.
pub fn left(length: usize, count: i32) -> Result<Range<usize>, PlanFailure> {
    if count < 0 {
        return Err(PlanFailure::Value);
    }
    Ok(0..length.min(count as usize))
}
/// Plan a clamped suffix range without allocating output.
pub fn right(length: usize, count: i32) -> Result<Range<usize>, PlanFailure> {
    if count < 0 {
        return Err(PlanFailure::Value);
    }
    Ok(length.saturating_sub(count as usize)..length)
}
/// Plan MID selection with the pinned Java signed-index behavior.
pub fn mid(length: usize, start: i32, count: i32) -> Result<Range<usize>, PlanFailure> {
    let start = start.wrapping_sub(1);
    if start < 0 || count < 0 {
        return Err(PlanFailure::Value);
    }
    if start as usize > length {
        return Ok(length..length);
    }
    let end = start
        .checked_add(count)
        .ok_or(PlanFailure::JavaIndexOverflow)?;
    Ok(start as usize..length.min(end as usize))
}
/// Exact-unit scan: combine only an adjacent high/low surrogate pair.
/// The caller admits scan work before invoking this allocation-free helper.
pub fn paired_length(text: &[u16]) -> usize {
    let mut offset = 0;
    let mut count = 0;
    while offset < text.len() {
        offset += pair_width(text, offset);
        count += 1;
    }
    count
}
fn pair_width(text: &[u16], offset: usize) -> usize {
    if text
        .get(offset)
        .is_some_and(|unit| (0xd800..=0xdbff).contains(unit))
        && text
            .get(offset + 1)
            .is_some_and(|u| (0xdc00..=0xdfff).contains(u))
    {
        2
    } else {
        1
    }
}
/// Convert a logical boundary to its exact UTF-16 offset, clamping past end.
pub fn paired_offset(text: &[u16], index: usize) -> usize {
    let mut offset = 0;
    for _ in 0..index {
        if offset == text.len() {
            break;
        }
        offset += pair_width(text, offset);
    }
    offset
}
/// Plan the replaced interval after one-based argument validation.
pub fn replace(length: usize, start: i32, count: i32) -> Result<Range<usize>, PlanFailure> {
    if start < 1 || count < 0 {
        return Err(PlanFailure::Value);
    }
    let start = (start as usize - 1).min(length);
    Ok(start..start.saturating_add(count as usize).min(length))
}
/// V2 boundaries clamp without importing Java MID's signed overflow fault.
pub fn paired_mid(length: usize, start: i32, count: i32) -> Result<Range<usize>, PlanFailure> {
    replace(length, start, count)
}
/// Java start_num - 1 wraps before its negative check.
pub fn find_start(start: i32) -> Result<usize, PlanFailure> {
    let start = start.wrapping_sub(1);
    if start < 0 {
        return Err(PlanFailure::Value);
    }
    Ok(start as usize)
}
/// Estimate UTF-16 output and work before case mapping.
pub fn case_reservation(input: usize) -> Result<Reservation, PlanFailure> {
    // Java helper owns code-point tuple and output vectors. Reserve both.
    let output = input.checked_mul(3).ok_or(PlanFailure::SizeOverflow)?;
    Reservation::units(
        output,
        input.checked_mul(32).ok_or(PlanFailure::SizeOverflow)?,
    )
}
/// Estimate text-search work and temporary storage before execution.
pub fn search_reservation(hay: usize, needle: usize) -> Result<Reservation, PlanFailure> {
    let inputs = hay.checked_add(needle).ok_or(PlanFailure::SizeOverflow)?;
    let output = inputs.checked_mul(3).ok_or(PlanFailure::SizeOverflow)?;
    // Worst-case literal window search plus mapping table work.
    let comparisons = hay
        .checked_mul(3)
        .and_then(|h| needle.checked_mul(3).and_then(|n| h.checked_mul(n)))
        .ok_or(PlanFailure::SizeOverflow)?;
    Reservation::units(
        output,
        inputs
            .checked_mul(32)
            .and_then(|n| n.checked_add(comparisons))
            .ok_or(PlanFailure::SizeOverflow)?,
    )
}
/// Validate repeated-text output size and return its admission plan.
pub fn rept(length: usize, count: f64) -> Result<(usize, Reservation), PlanFailure> {
    if count < 0. || length as f64 * count.floor() > 32767. {
        return Err(PlanFailure::Value);
    }
    // Java double-to-int conversion saturates, and NaN casts to zero.
    let count = count as i32;
    let count = count.max(0) as usize;
    let units = length.checked_mul(count).ok_or(PlanFailure::SizeOverflow)?;
    // Empty input with a very large count still needs loop-work admission.
    Ok((
        count,
        Reservation::units(
            units,
            units.checked_add(count).ok_or(PlanFailure::SizeOverflow)?,
        )?,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_emoji_ranges_preserve_surrogates() {
        let emoji = [0xd83d, 0xde00];
        assert_eq!(&emoji[left(2, 1).unwrap()], &[0xd83d]);
        assert_eq!(&emoji[right(2, 1).unwrap()], &[0xde00]);
        assert_eq!(&emoji[mid(2, 2, 1).unwrap()], &[0xde00]);
    }
    #[test]
    fn java_source_arithmetic_and_repeat_admission() {
        assert_eq!(mid(3, 2, i32::MAX), Err(PlanFailure::JavaIndexOverflow));
        assert_eq!(mid(3, i32::MAX, i32::MAX), Ok(3..3));
        assert_eq!(mid(3, i32::MIN, 1), Ok(3..3));
        assert_eq!(find_start(i32::MIN), Ok(i32::MAX as usize));
        assert_eq!(find_start(0), Err(PlanFailure::Value));
        assert_eq!(rept(2, 16384.), Err(PlanFailure::Value));
        assert_eq!(rept(2, 1.9).unwrap().0, 1);
        assert_eq!(rept(0, 1e30).unwrap().1.work, i32::MAX as usize);
    }
}

#[cfg(test)]
mod paired_tests {
    use super::*;
    #[test]
    fn exact_boundaries_preserve_isolated_surrogates_marks_and_selectors() {
        let text = [0xd800, 0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f];
        assert_eq!(paired_length(&text), 5);
        for (index, offset) in [0, 1, 3, 4, 5, 6, 6].into_iter().enumerate() {
            assert_eq!(paired_offset(&text, index), offset);
        }
        assert_eq!(paired_offset(&text, usize::MAX), 6);
        assert_eq!(paired_length(&[0xdc00, 0xd800]), 2);
        assert_eq!(replace(5, 0, 1), Err(PlanFailure::Value));
        assert_eq!(replace(5, 1, -1), Err(PlanFailure::Value));
        assert_eq!(replace(5, i32::MAX, i32::MAX), Ok(5..5));
        assert_eq!(paired_mid(5, 2, i32::MAX), Ok(1..5));
    }
}
