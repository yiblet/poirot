//! Versioned calculation-owned streams; no platform hashes or ambient RNG.
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Version of seed encoding and stream generation used for reproducible draws.
pub const STREAM_VERSION: u32 = 1;
/// Maximum generator words admitted for one unbiased integer draw.
pub const INTEGER_WORK: u64 = 128;
/// One cell's versioned random-stream consumption: seed, zero-based coordinates,
/// logical draws, and generator words consumed (including integer rejection samples).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RandomAccess {
    /// Captured evaluation seed.
    pub seed: u64,
    /// Zero-based workbook sheet index.
    pub sheet: u64,
    /// Zero-based formula row.
    pub row: u32,
    /// Zero-based formula column.
    pub column: u32,
    /// Stream algorithm and seed encoding version.
    pub version: u32,
    /// Number of logical random requests.
    pub draws: u64,
    /// Generator words consumed, including rejection samples.
    pub words: u64,
}
/// Reproducible generator owned by one evaluation cell.
pub(crate) struct Stream {
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
/// Inclusive integer interval whose members are exactly representable in binary64.
#[derive(Clone, Copy)]
pub(crate) struct Interval {
    bottom: i64,
    width: u64,
}
impl Interval {
    /// Validate integral ordered bounds within the supported exact-number envelope.
    pub fn new(bottom: f64, top: f64) -> Option<Self> {
        // Every integer in this envelope is exactly representable in binary64.
        const LIMIT: f64 = 9007199254740992.0;
        if !bottom.is_finite()
            || !top.is_finite()
            || bottom.fract() != 0.0
            || top.fract() != 0.0
            || bottom < -LIMIT
            || top > LIMIT
            || bottom > top
        {
            return None;
        }
        let bottom = bottom as i64;
        Some(Self {
            bottom,
            width: (top as i64 - bottom) as u64 + 1,
        })
    }
}
