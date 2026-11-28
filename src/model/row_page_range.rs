use super::CellAddress;
use super::cell_pages::{CellRange, CellStore, PageId};
use std::collections::btree_set;

/// A bounded page-index seek shared by rows in the same worksheet tile band.
#[derive(Clone)]
pub(crate) struct RowPageRange<'a> {
    store: &'a CellStore,
    first_col: u32,
    last_col: u32,
    band: Option<(u32, btree_set::Range<'a, PageId>)>,
}

impl<'a> RowPageRange<'a> {
    /// Keep the worksheet and the inclusive columns requested by each row.
    pub(super) fn new(store: &'a CellStore, first_col: u32, last_col: u32) -> Self {
        Self {
            store,
            first_col,
            last_col,
            band: None,
        }
    }

    /// Open one bounded row using the existing tile-band seek when possible.
    pub(crate) fn row(&mut self, row: u32, reverse: bool) -> CellRange<'a> {
        let band = self.store.page_id(CellAddress { row, col: 0 }).row_band();
        let pages = match &self.band {
            Some((current, pages)) if *current == band => pages.clone(),
            _ => {
                let pages = self.store.page_columns(band, self.first_col, self.last_col);
                self.band = Some((band, pages.clone()));
                pages
            }
        };
        CellRange::row_with_pages(
            self.store,
            row,
            self.first_col,
            self.last_col,
            reverse,
            pages,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Cell, CellAddress, CellStore};

    #[test]
    fn repeated_row_seeks_clip_both_tile_edges_across_bands() {
        let mut store = CellStore::<Cell>::default();
        for row in [30, 31, 32, 34] {
            for col in [0, 1, 2, 4, 5, 6, 7, 12] {
                store.insert(CellAddress { row, col }, Cell::default());
            }
        }
        let mut pages = store.cells().row_page_range(2, 5);
        for row in [30, 31, 32, 33, 34, 32, 31] {
            let forward: Vec<_> = pages.row(row, false).map(|(at, _)| at.col).collect();
            let reverse: Vec<_> = pages.row(row, true).rev().map(|(at, _)| at.col).collect();
            if row == 33 {
                assert!(forward.is_empty());
                assert!(reverse.is_empty());
            } else {
                assert_eq!(forward, [2, 4, 5]);
                assert_eq!(reverse, [5, 4, 2]);
            }
        }
    }
}
