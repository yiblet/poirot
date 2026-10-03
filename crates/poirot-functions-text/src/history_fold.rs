//! Standard Unicode16 simple folding, adapted from casefold0.1.0 (MIT).
//! Only the safe single-character lookup and generated tables are reused.
//! <https://github.com/github/rust-gems/blob/b8c6e27652f7f198a3a0f2183179948cd69be0c1/crates/casefold/src/simple_fold.rs>
//! Indexing and UTF-8 decoding use checked access; the bulk/unsafe APIs are omitted.
use std::cmp::Ordering;
#[path = "history_fold_data.rs"]
mod data;

/// Version of the case-insensitive comparison contract and embedded data.
pub const VERSION: &str = "unicode16-simple-fold-v1";

/// Apply one Unicode simple fold, without expansions or locale-specific mappings.
pub fn simple_fold(character: char) -> char {
    mapped(character).unwrap_or(character)
}

fn mapped(character: char) -> Option<char> {
    let cp = character as u32;
    if cp < 0x80 {
        return Some(character.to_ascii_lowercase());
    }
    let word_idx = (cp >> 12) as usize;
    let bit_idx = (cp >> 6) & 0x3f;
    let low = (cp & 0x3f) as u8;
    let word = *data::PAGE_BITMAP.get(word_idx)?;
    if word.checked_shr(bit_idx)? & 1 == 0 {
        return None;
    }
    let base = u32::from(*data::POPCNT_SAMPLES.get(word_idx)?);
    let mask = 1u64.checked_shl(bit_idx)?.wrapping_sub(1);
    // SAFETY: base came from a u16; a u64 popcount is at most 64, so the u32 sum cannot overflow.
    let dense = (base + (word & mask).count_ones()) as usize;
    let start = usize::from(*data::PAGE_OFFSET.get(dense)?);
    let end = usize::from(*data::PAGE_OFFSET.get(dense.checked_add(1)?)?);
    // A short ordinary scan replaces upstream's padded SWAR over-read.
    let offset = data::RUN_END_LOW
        .get(start..end)?
        .iter()
        .position(|&v| v >= low)?;
    let index = start.checked_add(offset)?;
    let stride = *data::RUN_START_STRIDE.get(index)?;
    let run_start = stride & 0x3f;
    // SAFETY: Short-circuit evaluation checks low >= run_start before subtraction.
    if low < run_start || ((low - run_start) & (stride >> 6)) != 0 {
        return None;
    }
    let mut bytes = [0u8; 4];
    // SAFETY: Four locally allocated bytes hold the UTF-8 encoding of any char.
    character.encode_utf8(&mut bytes);
    let folded = u32::from_le_bytes(bytes).wrapping_add(*data::BYTE_DELTA.get(index)?);
    let bytes = folded.to_le_bytes();
    // Upstream utf8_len's packed lead-byte lookup, applied to a local byte.
    // SAFETY: The u8 high nibble is 0..15; multiplication yields 0..60, below the u64 shift width.
    let length = ((0x4322_1111_1111_1111u64 >> (4 * (folded as u8 >> 4))) & 0xf) as usize;
    std::str::from_utf8(bytes.get(..length)?)
        .ok()?
        .chars()
        .next()
}

/// Compare UTF-8 strings after simple folding, without normalization or allocation.
pub fn compare(left: &str, right: &str) -> Ordering {
    left.chars()
        .map(simple_fold)
        .cmp(right.chars().map(simple_fold))
}

/// Fold decoded UTF-16 scalars while preserving each unpaired surrogate as itself.
pub fn utf16(units: impl IntoIterator<Item = u16>) -> impl Iterator<Item = u32> {
    char::decode_utf16(units).map(|point| match point {
        Ok(character) => simple_fold(character) as u32,
        Err(error) => u32::from(error.unpaired_surrogate()),
    })
}

/// Compare folded UTF-16 scalars, preserving unpaired units without replacement.
pub fn compare_utf16(
    left: impl IntoIterator<Item = u16>,
    right: impl IntoIterator<Item = u16>,
) -> Ordering {
    utf16(left).cmp(utf16(right))
}
