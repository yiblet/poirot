use std::{collections::BTreeMap, ops::Range};

/// Rebuilt backend ZIP with slices of original, fully expanded worksheets.
pub(crate) struct WorksheetArchive {
    archive: Vec<u8>,
    parts: BTreeMap<String, Range<usize>>,
}

impl WorksheetArchive {
    /// Keep rebased offsets with their owning bytes.
    pub(super) fn new(archive: Vec<u8>, parts: BTreeMap<String, Range<usize>>) -> Option<Self> {
        parts
            .values()
            .all(|range| archive.get(range.clone()).is_some())
            .then_some(Self { archive, parts })
    }

    /// Original XLSX content in the backend's ZIP container.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.archive
    }

    /// Original worksheet XML if its expansion was retained.
    pub(crate) fn worksheet(&self, part: &str) -> Option<&[u8]> {
        self.archive.get(self.parts.get(part)?.clone())
    }
}
