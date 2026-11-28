//! Canonical identities for equal normalized source style records.

use crate::{AllocationError, IoError, StyleDecodeError};
use std::collections::HashMap;
use std::hash::Hash;
use std::num::NonZeroUsize;

/// Maps admitted source indices to shared identities and borrowed component values.
pub(super) struct ComponentIds<'a, T> {
    entries: Vec<Option<(NonZeroUsize, &'a T)>>,
}

impl<'a, T: Eq + Hash> ComponentIds<'a, T> {
    /// Canonicalize visible component values while retaining their source indices.
    pub(super) fn new<S>(
        source: &'a [S],
        normalize: impl Fn(&'a S) -> Option<&'a T>,
    ) -> Result<Self, IoError> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(source.len())
            .map_err(|error| IoError::Allocation(AllocationError::StyleTable(error)))?;
        let mut distinct = HashMap::new();
        if source.len() > 1 {
            distinct
                .try_reserve(source.len())
                .map_err(|error| IoError::Allocation(AllocationError::SharedStyles(error)))?;
        }
        for (index, record) in source.iter().enumerate() {
            let entry = normalize(record).and_then(|value| {
                // SAFETY: enumerate yields integers below source.len(), so index + 1 fits.
                let id = NonZeroUsize::new(index + 1);
                let id = if source.len() > 1 {
                    *distinct.entry(value).or_insert(id)
                } else {
                    id
                };
                id.map(|id| (id, value))
            });
            // SAFETY: The capacity above covers exactly one push per source element.
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    /// Read a checked source slot or the implicit default component.
    pub(super) fn get(&self, index: usize) -> Result<Option<(NonZeroUsize, &'a T)>, IoError> {
        if self.entries.is_empty() && index == 0 {
            return Ok(None);
        }
        self.entries.get(index).copied().ok_or(IoError::Style(
            StyleDecodeError::DanglingStyleComponentIndex,
        ))
    }
}
