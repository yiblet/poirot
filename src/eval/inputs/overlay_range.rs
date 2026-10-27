//! Bounded merged base and overlay iteration for sparse range consumers.

use super::cell_edit::CellEdit;
use super::stored_source::StoredSource;
use crate::model::CellAddress;
use std::collections::btree_map;

/// Merged bounded base/overlay iterator used by sparse range consumers.
pub(super) enum OverlayRange<'a> {
    Base(crate::model::CellRange<'a>),
    Input(btree_map::Range<'a, CellAddress, CellEdit>),
}

impl<'a> Iterator for OverlayRange<'a> {
    /// Borrowed effective coordinate and cell after overlay precedence.
    type Item = (&'a CellAddress, StoredSource<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r
                .next()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell))),
            Self::Input(r) => r.next().map(|(at, edit)| (at, StoredSource::Edit(edit))),
        }
    }
}

impl DoubleEndedIterator for OverlayRange<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r
                .next_back()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell))),
            Self::Input(r) => r
                .next_back()
                .map(|(at, edit)| (at, StoredSource::Edit(edit))),
        }
    }
}

/// Step a merge cursor forward or backward, if one is present.
pub(super) fn advance<'a>(
    range: &mut Option<OverlayRange<'a>>,
    reverse: bool,
) -> Option<(&'a CellAddress, StoredSource<'a>)> {
    let range = range.as_mut()?;
    if reverse {
        range.next_back()
    } else {
        range.next()
    }
}
