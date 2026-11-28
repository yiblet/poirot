use super::cached::Cached;
use super::key::Key;
use super::range_node::RangeNode;
use super::range_purpose::RangePurpose;
use super::range_tree::RangeTree;
use crate::eval::coordinates::CellMap;
use std::collections::BTreeMap;

/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
#[derive(Clone, Default)]
pub(super) struct RangeIndex {
    pub(super) sheets: BTreeMap<usize, RangeTree>,
}

impl RangeIndex {
    pub(super) fn readers(&self, key: Key, out: &mut Vec<Key>) {
        if let Some(tree) = self.sheets.get(&key.0) {
            tree.readers(key.1, out);
        }
    }

    /// Whether every indexed rectangle on the sheet starts above and left of this point.
    pub(super) fn after_all_starts(&self, (sheet, address): Key) -> bool {
        self.sheets
            .get(&sheet)
            .is_none_or(|tree| tree.last_start.0 <= address.row && tree.last_start.1 <= address.col)
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
                    .nodes
                    .push(RangeNode::new(*range, (sheet, at)));
            }
        }
        for tree in index.sheets.values_mut() {
            tree.build();
        }
        index
    }
}
