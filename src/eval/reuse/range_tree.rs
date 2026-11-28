use super::key::Key;
use super::range_node::RangeNode;
use crate::model::CellAddress;

/// Finds readers of one sheet by searching a balanced tree in a contiguous vector.
/// Every slice's midpoint holds one rectangle and the bounds of both descendants.
#[derive(Clone, Default)]
pub(super) struct RangeTree {
    pub(super) nodes: Vec<RangeNode>,
    /// Greatest starting row and column across all indexed rectangles.
    pub(super) last_start: (u32, u32),
}

impl RangeTree {
    /// Collect matching readers in ascending start row order.
    pub(super) fn readers(&self, address: CellAddress, out: &mut Vec<Key>) {
        search(&self.nodes, address, out);
    }

    /// Order entries and compute two-dimensional subtree bounds.
    pub(super) fn build(&mut self) {
        self.last_start = (0, 0);
        for node in &self.nodes {
            self.last_start.0 = self.last_start.0.max(node.start.row);
            self.last_start.1 = self.last_start.1.max(node.start.col);
        }
        self.nodes.sort_unstable_by_key(|node| node.start.row);
        summarize(&mut self.nodes);
    }
}

/// Visit the ordered subtrees whose row and column bounds cover the queried cell.
fn search(nodes: &[RangeNode], at: CellAddress, out: &mut Vec<Key>) {
    let mid = nodes.len() / 2;
    let Some(node) = nodes.get(mid) else {
        return;
    };
    if at.row < node.first.row
        || at.row > node.last.row
        || at.col < node.first.col
        || at.col > node.last.col
    {
        return;
    }
    // SAFETY: The preceding get proves the midpoint is within this slice.
    let (left, rest) = nodes.split_at(mid);
    if let Some((_, right)) = rest.split_first() {
        if !left.is_empty() {
            search(left, at, out);
        }
        // A leaf's enclosing bounds are its own rectangle.
        if left.is_empty()
            || ((node.start.row..=node.end.row).contains(&at.row)
                && (node.start.col..=node.end.col).contains(&at.col))
        {
            out.push(node.reader);
        }
        if !right.is_empty() {
            search(right, at, out);
        }
    }
}

/// Build enclosing bounds bottom-up in the same layout used by lookup.
fn summarize(nodes: &mut [RangeNode]) -> Option<(CellAddress, CellAddress)> {
    let mid = nodes.len() / 2;
    // SAFETY: Dividing the slice length by two never exceeds that length.
    let (left, right) = nodes.split_at_mut(mid);
    let (node, right) = right.split_first_mut()?;
    for (first, last) in [summarize(left), summarize(right)].into_iter().flatten() {
        node.enclose(first, last);
    }
    Some((node.first, node.last))
}

#[cfg(test)]
mod tests {
    use super::{RangeNode, RangeTree};
    use crate::model::CellAddress;
    use crate::model::query::RangeMembership;

    #[test]
    fn subtree_pruning_matches_ordered_scan_on_both_axes() {
        for count in [0, 1, 2, 3, 7, 8, 31, 32, 127] {
            let mut tree = RangeTree::default();
            for index in 0..count {
                let first = CellAddress {
                    row: (index * 19) % 41,
                    col: (index * 31) % 61,
                };
                let last = CellAddress {
                    row: first.row + (index * 5) % 17,
                    col: first.col + (index * 7) % 23,
                };
                let range = RangeMembership::new(2, first, last).unwrap();
                let reader = (index as usize % 3, first);
                tree.nodes.push(RangeNode::new(range, reader));
            }
            tree.build();
            let mut actual = Vec::new();
            for row in 0..60 {
                for col in 0..90 {
                    let at = CellAddress { row, col };
                    let expected: Vec<_> = tree
                        .nodes
                        .iter()
                        .filter(|node| {
                            RangeMembership {
                                sheet: 2,
                                first: node.start,
                                last: node.end,
                            }
                            .contains(2, at)
                        })
                        .map(|node| node.reader)
                        .collect();
                    actual.clear();
                    tree.readers(at, &mut actual);
                    assert_eq!(actual, expected, "{count} entries at {at:?}");
                }
            }
        }
    }
}
