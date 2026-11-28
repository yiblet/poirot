//! Borrowed tiles for repeatedly reading an immutable worksheet.
use super::cell_pages::{CellStore, Page, PageId};
use super::{Cell, CellAddress};
use std::cell::Cell as Slot;

/// A few recently accessed tiles, tied to the immutable stores they came from.
#[derive(Default)]
pub(crate) struct CellPageCache<'a> {
    entries: [Slot<Option<CachedPage<'a>>>; 4],
}

/// A page lookup, including the absence of a tile at the selected coordinate.
#[derive(Clone, Copy)]
struct CachedPage<'a> {
    store: &'a CellStore,
    id: PageId,
    page: Option<&'a Page>,
}

impl<'a> CellPageCache<'a> {
    /// Read a cell, including misses within a previously looked-up tile.
    #[inline]
    pub(crate) fn get(&self, store: &'a CellStore, at: CellAddress) -> Option<&'a Cell> {
        let id = store.page_id(at);
        let index = id.cache_index();
        let entry = self.entries.get(index)?;
        let page = match entry.get() {
            Some(entry) if std::ptr::eq(entry.store, store) && entry.id == id => entry.page,
            _ => {
                let page = store.page(id);
                entry.set(Some(CachedPage { store, id, page }));
                page
            }
        };
        page?.get(at)
    }
}

#[cfg(test)]
mod tests {
    use super::{CellPageCache, CellStore};
    use crate::model::{Cell, CellAddress, CellValue};

    #[test]
    fn cache_preserves_empty_slots_missing_tiles_and_store_identity() {
        let at = CellAddress { row: 0, col: 0 };
        let empty_slot = CellAddress { row: 1, col: 0 };
        let missing_tile = CellAddress { row: 128, col: 0 };
        let mut first = CellStore::default();
        let second = CellStore::default();
        first.insert(
            at,
            Cell {
                value: CellValue::Number(1.0),
                ..Default::default()
            },
        );
        let cache = CellPageCache::default();

        assert_eq!(cache.get(&first, at).unwrap().value, CellValue::Number(1.0));
        assert!(cache.get(&first, empty_slot).is_none());
        assert!(cache.get(&second, at).is_none());
        assert_eq!(cache.get(&first, at).unwrap().value, CellValue::Number(1.0));
        assert!(cache.get(&first, missing_tile).is_none());
        assert!(cache.get(&first, missing_tile).is_none());
        assert_eq!(cache.get(&first, at).unwrap().value, CellValue::Number(1.0));
    }
}
