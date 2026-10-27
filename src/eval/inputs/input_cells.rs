//! One sheet's sparse exclusive edits.

use super::cell_edit::CellEdit;
use crate::model::{Cell, CellAddress, ModelError};
use std::collections::{BTreeMap, BTreeSet, btree_map};

/// One sheet's exclusive edits, kept sparse even for isolated deletions.
#[derive(Default)]
pub(in crate::eval) struct InputCells {
    pub(super) edits: BTreeMap<CellAddress, CellEdit>,
    /// Derived coordinate-only column order; edit meaning lives exclusively in `edits`.
    pub(super) columns: BTreeSet<(u32, u32)>,
}

impl InputCells {
    /// Validate and replace one edit without allocating a dense cell page.
    pub(in crate::eval) fn set(
        &mut self,
        address: CellAddress,
        edit: CellEdit,
    ) -> Result<Option<CellEdit>, ModelError> {
        address.validate()?;
        let old = self.edits.insert(address, edit);
        if old.is_none() {
            self.columns.insert((address.col, address.row));
        }
        Ok(old)
    }

    /// Store content, reusing its existing allocation; report whether an edit existed.
    pub(in crate::eval) fn set_cell(
        &mut self,
        address: CellAddress,
        cell: Cell,
        style_only: bool,
    ) -> Result<bool, ModelError> {
        address.validate()?;
        match self.edits.entry(address) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let previous = std::mem::replace(entry.get_mut(), CellEdit::Delete);
                let cell = match previous {
                    CellEdit::Replace(mut stored) | CellEdit::StyleOnly(mut stored) => {
                        *stored = cell;
                        stored
                    }
                    // SAFETY: Cell has a fixed-size Rust layout, so this box cannot overflow its allocation size.
                    CellEdit::Delete => Box::new(cell),
                };
                *entry.get_mut() = CellEdit::with_cell(cell, style_only);
                Ok(true)
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                // SAFETY: Cell has a fixed-size Rust layout, so this box cannot overflow its allocation size.
                entry.insert(CellEdit::with_cell(Box::new(cell), style_only));
                self.columns.insert((address.col, address.row));
                Ok(false)
            }
        }
    }

    /// Borrow an edit at its authored coordinate.
    pub(in crate::eval) fn get(&self, address: &CellAddress) -> Option<&CellEdit> {
        self.edits.get(address)
    }

    /// Mutably borrow an existing edit.
    pub(in crate::eval) fn get_mut(&mut self, address: &CellAddress) -> Option<&mut CellEdit> {
        self.edits.get_mut(address)
    }

    /// Remove one edit, restoring baseline inheritance.
    pub(in crate::eval) fn remove(&mut self, address: &CellAddress) -> Option<CellEdit> {
        let old = self.edits.remove(address);
        if old.is_some() {
            self.columns.remove(&(address.col, address.row));
        }
        old
    }

    /// Borrow the ordered edits for sparse traversal.
    pub(in crate::eval) fn cells(&self) -> &BTreeMap<CellAddress, CellEdit> {
        &self.edits
    }

    /// Borrow a bounded edit interval; reversed bounds produce an empty cursor.
    pub(super) fn range(
        &self,
        range: std::ops::RangeInclusive<CellAddress>,
    ) -> btree_map::Range<'_, CellAddress, CellEdit> {
        if range.start() > range.end() {
            // SAFETY: Equal included/excluded bounds form a valid empty interval.
            return self.edits.range(*range.start()..*range.start());
        }
        // SAFETY: The guard above excludes reversed inclusive bounds.
        self.edits.range(range)
    }
}
