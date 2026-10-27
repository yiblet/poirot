use super::interval::Interval;
use rand_chacha::ChaCha8Rng;
use rand_core::{Rng, SeedableRng};

/// Version of seed encoding and stream generation used for reproducible draws.
pub const STREAM_VERSION: u32 = 1;

/// Maximum generator words admitted for one unbiased integer draw.
pub const INTEGER_WORK: u64 = 128;

/// Reproducible generator owned by one evaluation cell.
pub(in crate::eval) struct Stream {
    rng: ChaCha8Rng,
}

impl Stream {
    /// Derive a stream from explicit seed and coordinates using versioned byte encoding.
    pub fn new(seed: u64, sheet: u64, row: u32, column: u32) -> Self {
        let mut bytes = [0; 32];
        // SAFETY: bytes has 32 bytes; 0..8 selects eight bytes, matching a u64 byte array.
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        // SAFETY: bytes has 32 bytes; 8..16 selects eight bytes, matching a u64 byte array.
        bytes[8..16].copy_from_slice(&sheet.to_le_bytes());
        // SAFETY: bytes has 32 bytes; 16..20 selects four bytes, matching a u32 byte array.
        bytes[16..20].copy_from_slice(&row.to_le_bytes());
        // SAFETY: bytes has 32 bytes; 20..24 selects four bytes, matching a u32 byte array.
        bytes[20..24].copy_from_slice(&column.to_le_bytes());
        // SAFETY: bytes has 32 bytes; its suffix from 24 and the literal each contain eight bytes.
        bytes[24..].copy_from_slice(b"ROIrand1");
        Self {
            rng: ChaCha8Rng::from_seed(bytes),
        }
    }

    /// Consume one 53-bit uniform draw in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.rng.next_u64() >> 11) as f64 * (1.0 / 9007199254740992.0)
    }

    /// Draw an unbiased interval member, returning generator-word consumption.
    pub fn integer(&mut self, interval: Interval) -> Option<(f64, u64)> {
        // Rejection removes modulo bias. At most 128 words, reserved before draw.
        let threshold = interval.width.wrapping_neg() % interval.width;
        for words in 1..=INTEGER_WORK {
            let word = self.rng.next_u64();
            if word >= threshold {
                return Some((
                    (interval.bottom + (word % interval.width) as i64) as f64,
                    words,
                ));
            }
        }
        None
    }
}
