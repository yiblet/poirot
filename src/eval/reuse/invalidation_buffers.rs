use super::key::Key;
use crate::eval::coordinates::CellSet;

/// Empty buffers retained between invalidation walks to avoid repeated growth.
#[derive(Default)]
pub(super) struct InvalidationBuffers {
    /// Coordinates already scheduled by the current walk.
    pub(super) seen: CellSet,
    /// Recent results from point and value-range reverse indexes.
    pub(super) readers: Vec<Key>,
    /// Recent results from the spill geometry index.
    pub(super) geometry: Vec<Key>,
}

impl InvalidationBuffers {
    /// Drop all contents and discard allocations too large to keep between edits.
    pub(super) fn clear(&mut self) {
        // Only small spare buffers persist; a large invalidation may still grow
        // to any size needed for the affected dependency graph during the edit.
        const MAX_RETAINED_CAPACITY: usize = 4096;

        if self.seen.capacity() > MAX_RETAINED_CAPACITY {
            self.seen = CellSet::default();
        } else {
            self.seen.clear();
        }
        for buffer in [&mut self.readers, &mut self.geometry] {
            if buffer.capacity() > MAX_RETAINED_CAPACITY {
                *buffer = Vec::new();
            } else {
                buffer.clear();
            }
        }
    }
}

impl Clone for InvalidationBuffers {
    fn clone(&self) -> Self {
        // Spare storage belongs to the workspace that used it.
        Self::default()
    }
}
