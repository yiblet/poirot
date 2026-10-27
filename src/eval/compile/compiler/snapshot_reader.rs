//! Bounds-checked reads of serialized compiled-program fields.

use super::plan_snapshot_error::PlanSnapshotError;

/// Bounds-checked reader for serialized little-endian reference fields.
pub(super) struct SnapshotReader<'a>(&'a [u8]);

impl<'a> SnapshotReader<'a> {
    pub(super) fn new(b: &'a [u8]) -> Self {
        Self(b)
    }

    /// Borrow the bytes not yet read.
    pub(super) fn remaining(&self) -> &'a [u8] {
        self.0
    }

    pub(super) fn word(&mut self) -> Result<u32, PlanSnapshotError> {
        let b = self
            .0
            .get(..4)
            .ok_or(PlanSnapshotError::SnapshotTruncatedReference)?;
        // SAFETY: The local get(..4) succeeds before conversion and advancement, proving four bytes are present.
        let n = u32::from_le_bytes(b.try_into().unwrap());
        // SAFETY: The local get(..4) succeeds before conversion and advancement, proving four bytes are present.
        self.0 = &self.0[4..];
        Ok(n)
    }

    pub(super) fn boolean(&mut self) -> Result<bool, PlanSnapshotError> {
        match self.word()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(PlanSnapshotError::SnapshotReferenceBoolean),
        }
    }

    pub(super) fn string(&mut self) -> Result<&'a str, PlanSnapshotError> {
        let n = self.word()? as usize;
        let b = self
            .0
            .get(..n)
            .ok_or(PlanSnapshotError::SnapshotTruncatedString)?;
        // SAFETY: The local get(..n) succeeds before advancing the same byte slice by n.
        self.0 = &self.0[n..];
        std::str::from_utf8(b).map_err(|_| PlanSnapshotError::SnapshotReferenceUTF8)
    }

    pub(super) fn optional(&mut self) -> Result<(), PlanSnapshotError> {
        if self.boolean()? {
            self.string()?;
        }
        Ok(())
    }

    pub(super) fn cell(&mut self) -> Result<(), PlanSnapshotError> {
        if self.word()? >= crate::model::MAX_ROWS || self.word()? >= crate::model::MAX_COLUMNS {
            return Err(PlanSnapshotError::SnapshotReferenceCoordinate);
        }
        self.boolean()?;
        self.boolean()?;
        self.optional()
    }
}
