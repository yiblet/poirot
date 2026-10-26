//! Validated worksheet coordinates and A1 notation.
use crate::ModelError;
use std::{fmt, str::FromStr};

/// Exclusive worksheet row bound shared by addresses and adapters.
pub const MAX_ROWS: u32 = 1_048_576;
/// Exclusive worksheet column bound shared by addresses and adapters.
pub const MAX_COLUMNS: u32 = 16_384;

/// A cell position with zero-based row and column indices.
///
/// Parse A1 notation with `"B3".parse()`, or use [`Self::new`] with `(2, 1)`.
/// The address is worksheet-local: the containing worksheet or an evaluator target
/// supplies sheet identity. It carries no formula-copy anchoring such as `$B$3`.
/// Public fields also allow unchecked construction; [`Self::validate`] checks bounds.
///
/// ```rust
/// use poirot_io::CellAddress;
/// let first: CellAddress = "$B$3".parse()?;
/// // Generate the next sample coordinate using zero-based indices.
/// let next = CellAddress::new(first.row + 1, first.col)?;
/// assert_eq!(next.to_string(), "B4");
/// assert!("XFE1".parse::<CellAddress>().is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct CellAddress {
    /// Zero-based row coordinate.
    pub row: u32,
    /// Zero-based column coordinate.
    pub col: u32,
}
impl CellAddress {
    /// Construct an address only if both axes fit worksheet bounds.
    pub fn new(row: u32, col: u32) -> Result<Self, ModelError> {
        let a = Self { row, col };
        a.validate()?;
        Ok(a)
    }
    /// Check that the row and column fit within Excel worksheet limits.
    pub fn validate(self) -> Result<(), ModelError> {
        if self.row >= MAX_ROWS || self.col >= MAX_COLUMNS {
            Err(ModelError(
                "cell address exceeds Excel worksheet limits".into(),
            ))
        } else {
            Ok(())
        }
    }
}
impl fmt::Display for CellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut col = u64::from(self.col) + 1;
        let mut letters = Vec::new();
        while col > 0 {
            letters.push((b'A' + ((col - 1) % 26) as u8) as char);
            col = (col - 1) / 26;
        }
        for c in letters.iter().rev() {
            write!(f, "{c}")?;
        }
        write!(f, "{}", u64::from(self.row) + 1)
    }
}
impl FromStr for CellAddress {
    /// Model diagnostic for invalid A1 syntax or worksheet bounds.
    type Err = ModelError;
    fn from_str(s: &str) -> Result<Self, ModelError> {
        let invalid = || ModelError(format!("invalid A1 cell address: {s}"));
        let s = s.strip_prefix('$').unwrap_or(s);
        let split = s
            .find(|c: char| !c.is_ascii_alphabetic())
            .ok_or_else(invalid)?;
        if split == 0 {
            return Err(invalid());
        }
        let mut col = 0u32;
        // SAFETY: split comes from str::find on a character predicate, so it is an in-bounds UTF-8 boundary.
        for b in s[..split].bytes() {
            col = col
                .checked_mul(26)
                .and_then(|v| v.checked_add(u32::from(b.to_ascii_uppercase() - b'A') + 1))
                .ok_or_else(invalid)?;
        }
        // SAFETY: split comes from str::find on a character predicate, so it is an in-bounds UTF-8 boundary.
        let row_text = s[split..].strip_prefix('$').unwrap_or(&s[split..]);
        if row_text.is_empty() || !row_text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let row: u32 = row_text.parse().map_err(|_| invalid())?;
        Self::new(row.checked_sub(1).ok_or_else(invalid)?, col - 1)
    }
}
