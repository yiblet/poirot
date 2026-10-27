//! A merge cursor's borrowed source before tombstone filtering.

use super::cell_edit::CellEdit;
use crate::model::Cell;

/// Borrowed source selected by a merge cursor before tombstone filtering.
#[derive(Clone, Copy)]
pub(super) enum StoredSource<'a> {
    Baseline(&'a Cell),
    Edit(&'a CellEdit),
}

impl<'a> StoredSource<'a> {
    /// The effective cell, or `None` for a deletion marker.
    pub(super) fn cell(self) -> Option<&'a Cell> {
        match self {
            Self::Baseline(cell) => Some(cell),
            Self::Edit(edit) => edit.cell(),
        }
    }
}
