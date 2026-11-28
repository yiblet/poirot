use super::zip_segments::ZipSegments;
use std::{collections::BTreeMap, ops::Range};

/// Retain checked worksheet XML in the suffix of a ZIP with an unchanged borrowed prefix.
pub(crate) struct AppendedWorksheets<'a> {
    pub(super) prefix: &'a [u8],
    pub(super) suffix: Vec<u8>,
    pub(super) worksheets: BTreeMap<String, Range<usize>>,
}

impl AppendedWorksheets<'_> {
    /// Read the transformed archive through ordinary backend ZIP APIs.
    pub(crate) fn reader(&self) -> ZipSegments<'_> {
        ZipSegments::new(self.prefix, &self.suffix)
    }

    /// Borrow the original checked XML without opening or inflating a second entry.
    pub(crate) fn worksheet(&self, name: &str) -> Option<&[u8]> {
        self.suffix.get(self.worksheets.get(name)?.clone())
    }
}
