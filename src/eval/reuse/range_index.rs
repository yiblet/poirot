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
}
