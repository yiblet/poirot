use super::cached::Cached;
use super::key::Key;
use super::range_purpose::RangePurpose;
use super::range_tree::RangeTree;
use crate::eval::coordinates::CellMap;
use std::collections::BTreeMap;

/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
#[derive(Default)]
pub(super) struct RangeIndex {
    pub(super) sheets: BTreeMap<usize, RangeTree>,
}

impl RangeIndex {
    #[cfg(test)]
    pub(super) fn intersects(&self, key: Key) -> bool {
        self.sheets.get(&key.0).is_some_and(|tree| {
            key.1.col >= tree.first_col
                && key.1.col <= tree.last_col
                && tree.intersects(1, 0, tree.entries.len(), key)
        })
    }

    pub(super) fn readers(&self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get(&key.0)
            && key.1.col >= tree.first_col
            && key.1.col <= tree.last_col
        {
            tree.readers(1, 0, tree.entries.len(), key, out);
        }
    }

    pub(super) fn new(entries: &CellMap<Cached>) -> Self {
        Self::build(entries, RangePurpose::Values)
    }

    pub(super) fn build(entries: &CellMap<Cached>, purpose: RangePurpose) -> Self {
        let mut index = Self::default();
        for (&(sheet, at), entry) in entries {
            let (ranges, points) = if !matches!(purpose, RangePurpose::Values) {
                (entry.dependencies.spill_ranges.as_slice(), &[][..])
            } else {
                (
                    entry.dependencies.ranges.as_slice(),
                    entry.dependencies.point_runs.as_slice(),
                )
            };
            for range in ranges.iter().chain(points) {
                index
                    .sheets
                    .entry(range.sheet)
                    .or_default()
                    .entries
                    .push((*range, (sheet, at)));
            }
        }
        for tree in index.sheets.values_mut() {
            (tree.first_col, tree.last_col) = tree
                .entries
                .iter()
                .fold((u32::MAX, 0), |(first, last), (range, _)| {
                    (first.min(range.first.col), last.max(range.last.col))
                });
            tree.entries
                .sort_unstable_by_key(|(range, _)| range.first.row);
            tree.max_end.resize(tree.entries.len() * 4, None);
            tree.build(1, 0, tree.entries.len());
        }
        index
    }

    // A matched entry can be consumed: its consumer is now pending and will
    // propagate once through `seen`, even if other entries reach it again.
    // Unmatched entries remain live, including column misses and row-zero reads.
    // Invalidation removes every reached consumer, marking the index dirty; the
    // next nonempty pass rebuilds only surviving traces. Exclusive mutable access
    // keeps a pass synchronous, so no nested pass can observe a partial closure.
    pub(super) fn take_readers(&mut self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get_mut(&key.0)
            && tree.first_col <= key.1.col
            && key.1.col <= tree.last_col
        {
            tree.query(1, 0, tree.entries.len(), key, out);
        }
    }
}
