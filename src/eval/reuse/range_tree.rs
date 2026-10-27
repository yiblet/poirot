use super::key::Key;
use crate::model::query::RangeMembership;

/// Finds cached consumers whose read rectangles include an edited cell on one sheet.
/// Row-sorted entries and subtree bounds prune unrelated rectangles; matched entries
/// can be consumed as invalidation advances through the dependency graph.
#[derive(Default)]
pub(super) struct RangeTree {
    pub(super) entries: Vec<(RangeMembership, Key)>,
    pub(super) max_end: Vec<Option<u32>>,
    pub(super) first_col: u32,
    pub(super) last_col: u32,
}

impl RangeTree {
    #[cfg(test)]
    pub(super) fn intersects(&self, node: usize, lo: usize, hi: usize, key: Key) -> bool {
        let Some((range, _)) = self.entries.get(lo) else {
            return false;
        };
        if lo >= hi
            || hi > self.entries.len()
            || range.first.row > key.1.row
            || self
                .max_end
                .get(node)
                .copied()
                .flatten()
                .is_none_or(|end| end < key.1.row)
        {
            return false;
        }
        // SAFETY: The preceding guard establishes lo < hi <= entries.len().
        if hi - lo == 1 {
            return range.contains(key.0, key.1);
        }
        // SAFETY: lo < hi proves hi - lo is defined and the midpoint cannot exceed hi.
        let mid = lo + (hi - lo) / 2;
        let Some(left) = node.checked_mul(2) else {
            return false;
        };
        let Some(right) = left.checked_add(1) else {
            return false;
        };
        self.intersects(left, lo, mid, key) || self.intersects(right, mid, hi, key)
    }

    pub(super) fn readers(&self, node: usize, lo: usize, hi: usize, key: Key, out: &mut Vec<Key>) {
        let Some((range, reader)) = self.entries.get(lo) else {
            return;
        };
        if lo >= hi
            || hi > self.entries.len()
            || range.first.row > key.1.row
            || self
                .max_end
                .get(node)
                .copied()
                .flatten()
                .is_none_or(|end| end < key.1.row)
        {
            return;
        }
        // SAFETY: The preceding guard establishes lo < hi <= entries.len().
        if hi - lo == 1 {
            if range.contains(key.0, key.1) {
                out.push(*reader);
            }
            return;
        }
        // SAFETY: lo < hi proves hi - lo is defined and the midpoint cannot exceed hi.
        let mid = lo + (hi - lo) / 2;
        let Some(left) = node.checked_mul(2) else {
            return;
        };
        let Some(right) = left.checked_add(1) else {
            return;
        };
        self.readers(left, lo, mid, key, out);
        self.readers(right, mid, hi, key, out);
    }

    pub(super) fn build(&mut self, node: usize, lo: usize, hi: usize) -> Option<u32> {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len() {
            return None;
        }
        if hi - lo == 1 {
            // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
            self.max_end[node] = Some(self.entries[lo].0.last.row);
        } else {
            let mid = (lo + hi) / 2;
            // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
            self.max_end[node] =
                self.build(node * 2, lo, mid)
                    .max(self.build(node * 2 + 1, mid, hi));
        }
        // SAFETY: The entry guard checks lo < hi <= entries.len() and node < max_end.len(); these vectors are not resized during this invocation.
        self.max_end[node]
    }

    pub(super) fn query(
        &mut self,
        node: usize,
        lo: usize,
        hi: usize,
        key: Key,
        out: &mut Vec<Key>,
    ) {
        if lo >= hi || hi > self.entries.len() || node >= self.max_end.len()
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            || self.entries[lo].0.first.row > key.1.row || self.max_end[node].is_none_or(|end| end < key.1.row)
        {
            return;
        }
        if hi - lo == 1 {
            // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
            if self.entries[lo].0.contains(key.0, key.1) {
                // SAFETY: Short-circuit entry checks require lo < hi <= entries.len() and node < max_end.len() before either index is read.
                out.push(self.entries[lo].1);
                // SAFETY: The entry guard checks node < max_end.len(); no vector is resized here.
                self.max_end[node] = None;
            }
        } else {
            let mid = (lo + hi) / 2;
            self.query(node * 2, lo, mid, key, out);
            self.query(node * 2 + 1, mid, hi, key, out);
            let left = self.max_end.get(node * 2).copied().flatten();
            let right = self.max_end.get(node * 2 + 1).copied().flatten();
            // SAFETY: The entry guard checks node < max_end.len(); recursive calls do not resize it.
            self.max_end[node] = left.max(right);
        }
    }
}
