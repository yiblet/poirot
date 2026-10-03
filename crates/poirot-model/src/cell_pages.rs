//! Generic sparse tiles with column-major payloads. Model cells default to 32x4.
use crate::{Cell, CellAddress};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, btree_map, btree_set},
    ops::{Bound, RangeBounds},
};
const ROW_BLOCK_HEIGHT: u32 = 64;
// Each row has at most MAX_COLUMNS stored cells, including explicit blanks.
// SAFETY: This assertion is evaluated at compile time; an invalid column configuration cannot produce a running program.
const _: () = assert!(crate::MAX_COLUMNS <= u16::MAX as u32);
/// Small row-indexed block retaining sparse cells and occupancy counts.
#[derive(Debug, Clone)]
struct RowBlock {
    mask: u64,
    counts: [u16; ROW_BLOCK_HEIGHT as usize],
}
impl Default for RowBlock {
    fn default() -> Self {
        Self {
            mask: 0,
            counts: [0; ROW_BLOCK_HEIGHT as usize],
        }
    }
}
/// Physical page coordinate independent of logical worksheet cell addresses.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct PageId {
    row: u32,
    col: u32,
}
impl PageId {
    fn of<const HEIGHT: u32, const WIDTH: u32>(address: CellAddress) -> Self {
        Self {
            row: address.row / HEIGHT,
            col: address.col / WIDTH,
        }
    }
}
fn offset<const HEIGHT: u32, const WIDTH: u32>(address: CellAddress) -> usize {
    (address.col % WIDTH) as usize * HEIGHT as usize + (address.row % HEIGHT) as usize
}
/// Sparse cell page storing occupancy and payloads for bounded local coordinates.
#[derive(Debug, Clone)]
struct Page<T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    slots: Box<[Option<(CellAddress, T)>]>,
    len: usize,
}
impl<T, const HEIGHT: u32, const WIDTH: u32> Page<T, HEIGHT, WIDTH> {
    fn new() -> Self {
        Self {
            slots: std::iter::repeat_with(|| None)
                .take(HEIGHT as usize * WIDTH as usize)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            len: 0,
        }
    }
}
/// Generic sparse paged storage shared by workbook cells and overlay payloads.
#[derive(Debug, Clone)]
pub struct CellStore<T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    pages: HashMap<PageId, Page<T, HEIGHT, WIDTH>, foldhash::fast::RandomState>,
    // Ordered scans retain only IDs, not payloads. This costs another tree entry
    // per page, plus the hash table's spare capacity; point reads avoid that tree.
    page_ids: BTreeSet<PageId>,
    // A block stores 136 bytes even for one isolated row, versus one usize
    // count per row. Dense rows share a tree entry and scan set bits;
    // isolated rows trade extra count storage for the same sparse block index.
    occupied_rows: BTreeMap<u32, RowBlock>,
    // Column-first tile keys let vertical ranges seek their column's pages
    // without scanning pages belonging to unrelated columns.
    column_pages: BTreeSet<(u32, u32)>,
    len: usize,
}
impl<T, const HEIGHT: u32, const WIDTH: u32> Default for CellStore<T, HEIGHT, WIDTH> {
    fn default() -> Self {
        // SAFETY: The assertion runs at compile time; invalid dimensions fail compilation.
        const {
            assert!(
                HEIGHT > 0 && WIDTH > 0 && HEIGHT <= crate::MAX_ROWS && WIDTH <= crate::MAX_COLUMNS
            )
        };
        Self {
            pages: HashMap::default(),
            page_ids: BTreeSet::new(),
            occupied_rows: BTreeMap::new(),
            column_pages: BTreeSet::new(),
            len: 0,
        }
    }
}
impl<T, const HEIGHT: u32, const WIDTH: u32> CellStore<T, HEIGHT, WIDTH> {
    /// Validate a coordinate before storing a payload. Missing coordinates allocate nothing.
    pub fn set(&mut self, address: CellAddress, value: T) -> Result<Option<T>, crate::ModelError> {
        address.validate()?;
        Ok(self.insert(address, value))
    }

    /// Borrow a payload at a coordinate without allocating a page.
    pub fn get(&self, address: &CellAddress) -> Option<&T> {
        self.pages
            .get(&PageId::of::<HEIGHT, WIDTH>(*address))?
            .slots
            .get(offset::<HEIGHT, WIDTH>(*address))?
            .as_ref()
            .map(|(_, cell)| cell)
    }
    /// Borrow an existing payload mutably.
    pub fn get_mut(&mut self, address: &CellAddress) -> Option<&mut T> {
        self.pages
            .get_mut(&PageId::of::<HEIGHT, WIDTH>(*address))?
            .slots
            .get_mut(offset::<HEIGHT, WIDTH>(*address))?
            .as_mut()
            .map(|(_, cell)| cell)
    }
    /// Store a payload and update page/row occupancy, returning any replaced value.
    pub(crate) fn insert(&mut self, address: CellAddress, cell: T) -> Option<T> {
        let id = PageId::of::<HEIGHT, WIDTH>(address);
        let page = self.pages.entry(id).or_insert_with(|| {
            self.page_ids.insert(id);
            self.column_pages.insert((id.col, id.row));
            Page::new()
        });
        let old = page
            .slots
            .get_mut(offset::<HEIGHT, WIDTH>(address))?
            .replace((address, cell))
            .map(|(_, cell)| cell);
        if old.is_none() {
            page.len += 1;
            self.len += 1;
            let block = self
                .occupied_rows
                .entry(address.row / ROW_BLOCK_HEIGHT)
                .or_default();
            let row = address.row % ROW_BLOCK_HEIGHT;
            // SAFETY: row is address.row modulo ROW_BLOCK_HEIGHT, and counts has exactly ROW_BLOCK_HEIGHT entries.
            block.counts[row as usize] += 1;
            block.mask |= 1u64 << row;
        }
        old
    }
    /// Remove a payload and release empty storage blocks.
    pub fn remove(&mut self, address: &CellAddress) -> Option<T> {
        let id = PageId::of::<HEIGHT, WIDTH>(*address);
        let page = self.pages.get_mut(&id)?;
        let (_, cell) = page
            .slots
            .get_mut(offset::<HEIGHT, WIDTH>(*address))?
            .take()?;
        page.len -= 1;
        self.len -= 1;
        if page.len == 0 {
            self.pages.remove(&id);
            self.page_ids.remove(&id);
            self.column_pages.remove(&(id.col, id.row));
        }
        let block_id = address.row / ROW_BLOCK_HEIGHT;
        let Some(block) = self.occupied_rows.get_mut(&block_id) else {
            return Some(cell);
        };
        let row = address.row % ROW_BLOCK_HEIGHT;
        // SAFETY: row is address.row modulo ROW_BLOCK_HEIGHT, and counts has exactly ROW_BLOCK_HEIGHT entries.
        block.counts[row as usize] -= 1;
        // SAFETY: row is address.row modulo ROW_BLOCK_HEIGHT, and counts has exactly ROW_BLOCK_HEIGHT entries.
        if block.counts[row as usize] == 0 {
            block.mask &= !(1u64 << row);
            if block.mask == 0 {
                self.occupied_rows.remove(&block_id);
            }
        }
        Some(cell)
    }
    /// Borrow the coordinate-ordered facade over paged storage.
    pub fn cells(&self) -> WorksheetCells<'_, T, HEIGHT, WIDTH> {
        WorksheetCells { store: self }
    }
}
impl<T: PartialEq, const HEIGHT: u32, const WIDTH: u32> PartialEq for CellStore<T, HEIGHT, WIDTH> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.cells().iter().eq(other.cells().iter())
    }
}
/// Ordered read-only view independent of the worksheet's payload layout.
#[derive(Debug)]
pub struct WorksheetCells<'a, T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    store: &'a CellStore<T, HEIGHT, WIDTH>,
}
impl<T, const HEIGHT: u32, const WIDTH: u32> Copy for WorksheetCells<'_, T, HEIGHT, WIDTH> {}
impl<T, const HEIGHT: u32, const WIDTH: u32> Clone for WorksheetCells<'_, T, HEIGHT, WIDTH> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> WorksheetCells<'a, T, HEIGHT, WIDTH> {
    /// Count stored coordinates, including explicit blanks.
    pub fn len(self) -> usize {
        self.store.len
    }
    /// Whether no coordinates are explicitly stored.
    pub fn is_empty(self) -> bool {
        self.store.len == 0
    }
    /// Iterate stored coordinates and payloads in row-major order.
    pub fn iter(self) -> CellIter<'a, T, HEIGHT, WIDTH> {
        CellIter {
            range: self.range(..),
            remaining: self.len(),
        }
    }
    /// Iterate stored coordinates in row-major order.
    pub fn keys(self) -> impl DoubleEndedIterator<Item = &'a CellAddress> + ExactSizeIterator {
        self.iter().map(|(a, _)| a)
    }
    /// Iterate payloads in row-major coordinate order.
    pub fn values(self) -> impl DoubleEndedIterator<Item = &'a T> + ExactSizeIterator {
        self.iter().map(|(_, c)| c)
    }
    /// Borrow one tile at a time; slots in a column are consecutive in memory.
    pub fn column(self, column: u32, first: u32, last: u32) -> ColumnCells<'a, T, HEIGHT, WIDTH> {
        // SAFETY: This is evaluated at compile time; zero storage dimensions cannot produce a running program.
        const {
            assert!(WIDTH > 0 && HEIGHT > 0);
        }
        let pages = if first <= last {
            // SAFETY: The tuple's first coordinate is fixed; division by positive HEIGHT preserves ordered row bounds.
            self.store
                .column_pages
                .range((column / WIDTH, first / HEIGHT)..=(column / WIDTH, last / HEIGHT))
        } else {
            // SAFETY: Equal included/excluded bounds form an empty, ordered range.
            self.store.column_pages.range((0, 0)..(0, 0))
        };
        ColumnCells {
            store: self.store,
            pages,
            column,
            next: first,
            last,
            page: None,
        }
    }
    /// Iterate only rows containing stored payloads.
    pub fn occupied_rows(self, first: u32, last: u32) -> OccupiedRows<'a> {
        OccupiedRows::new(self.store, first, last)
    }
    /// Seek directly to stored coordinates inside an inclusive address interval.
    pub fn range(self, bounds: impl RangeBounds<CellAddress>) -> CellRange<'a, T, HEIGHT, WIDTH> {
        CellRange::new(self.store, bounds)
    }
}
impl<T: PartialEq, const HEIGHT: u32, const WIDTH: u32> PartialEq
    for WorksheetCells<'_, T, HEIGHT, WIDTH>
{
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> IntoIterator
    for WorksheetCells<'a, T, HEIGHT, WIDTH>
{
    /// Borrowed coordinate and generic payload in row-major order.
    type Item = (&'a CellAddress, &'a T);
    /// Ordered iterator over the borrowed cell-storage view.
    type IntoIter = CellIter<'a, T, HEIGHT, WIDTH>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
/// Exact-size iterator over stored cells in row/column order. Missing cells are skipped;
/// forward and backward iteration share the same remaining range.
pub struct CellIter<'a, T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    range: CellRange<'a, T, HEIGHT, WIDTH>,
    remaining: usize,
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> Iterator for CellIter<'a, T, HEIGHT, WIDTH> {
    /// Borrowed stored coordinate and payload.
    type Item = (&'a CellAddress, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.range.next()?;
        self.remaining -= 1;
        Some(value)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<T, const HEIGHT: u32, const WIDTH: u32> DoubleEndedIterator
    for CellIter<'_, T, HEIGHT, WIDTH>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let value = self.range.next_back()?;
        self.remaining -= 1;
        Some(value)
    }
}
impl<T, const HEIGHT: u32, const WIDTH: u32> ExactSizeIterator for CellIter<'_, T, HEIGHT, WIDTH> {}
/// Iterator state that traverses occupied columns in one storage row.
struct RowCursor<'a, T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    store: &'a CellStore<T, HEIGHT, WIDTH>,
    row: u32,
    pages: btree_set::Range<'a, PageId>,
    page: Option<(&'a PageId, &'a Page<T, HEIGHT, WIDTH>)>,
    col: i32,
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> RowCursor<'a, T, HEIGHT, WIDTH> {
    fn new(
        store: &'a CellStore<T, HEIGHT, WIDTH>,
        row: u32,
        first_col: u32,
        last_col: u32,
        reverse: bool,
    ) -> Self {
        // SAFETY: This is evaluated at compile time; zero storage dimensions cannot produce a running program.
        const {
            assert!(WIDTH > 0 && HEIGHT > 0);
        }
        let first = first_col / WIDTH;
        let last = last_col / WIDTH;
        let col = if reverse { last_col } else { first_col };
        Self {
            store,
            row,
            pages: if first <= last {
                // SAFETY: first <= last was checked locally and both PageIds use the same row.
                store.page_ids.range(
                    PageId {
                        row: row / HEIGHT,
                        col: first,
                    }..=PageId {
                        row: row / HEIGHT,
                        col: last,
                    },
                )
            } else {
                // SAFETY: Equal included/excluded bounds form an empty, ordered range.
                store
                    .page_ids
                    .range(PageId { row: 0, col: 0 }..PageId { row: 0, col: 0 })
            },
            page: None,
            col: col as i32,
        }
    }
    fn advance(&mut self, reverse: bool) -> Option<(&'a CellAddress, &'a T)> {
        loop {
            if let Some((id, page)) = self.page {
                let base = id.col * WIDTH;
                while self.col >= base as i32 && self.col < (base + WIDTH) as i32 {
                    let col = self.col as u32;
                    self.col += if reverse { -1 } else { 1 };
                    let index =
                        (col % WIDTH) as usize * HEIGHT as usize + (self.row % HEIGHT) as usize;
                    if let Some((a, c)) = page.slots.get(index).and_then(Option::as_ref) {
                        return Some((a, c));
                    }
                }
            }
            let next = if reverse {
                self.pages.next_back()
            } else {
                self.pages.next()
            }?;
            let base = (next.col * WIDTH) as i32;
            self.col = if reverse {
                self.col.min(base + WIDTH as i32 - 1)
            } else {
                self.col.max(base)
            };
            self.page = Some((next, self.store.pages.get(next)?));
        }
    }
}
/// Lexicographically ordered sparse address range; mixed front/back reads do not repeat.
pub struct CellRange<'a, T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    store: &'a CellStore<T, HEIGHT, WIDTH>,
    first: Option<CellAddress>,
    last: Option<CellAddress>,
    forward: Option<RowCursor<'a, T, HEIGHT, WIDTH>>,
    backward: Option<RowCursor<'a, T, HEIGHT, WIDTH>>,
}
fn after(a: CellAddress) -> Option<CellAddress> {
    if a.col + 1 < crate::MAX_COLUMNS {
        Some(CellAddress {
            row: a.row,
            col: a.col + 1,
        })
    } else if a.row + 1 < crate::MAX_ROWS {
        Some(CellAddress {
            row: a.row + 1,
            col: 0,
        })
    } else {
        None
    }
}
fn before(a: CellAddress) -> Option<CellAddress> {
    if a.col > 0 {
        Some(CellAddress {
            row: a.row,
            col: a.col - 1,
        })
    } else if a.row > 0 {
        Some(CellAddress {
            row: a.row - 1,
            col: crate::MAX_COLUMNS - 1,
        })
    } else {
        None
    }
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> CellRange<'a, T, HEIGHT, WIDTH> {
    fn new(store: &'a CellStore<T, HEIGHT, WIDTH>, bounds: impl RangeBounds<CellAddress>) -> Self {
        let first = match bounds.start_bound() {
            Bound::Included(a) => Some(*a),
            Bound::Excluded(a) => after(*a),
            Bound::Unbounded => Some(CellAddress { row: 0, col: 0 }),
        };
        let last = match bounds.end_bound() {
            Bound::Included(a) => Some(*a),
            Bound::Excluded(a) => before(*a),
            Bound::Unbounded => Some(CellAddress {
                row: crate::MAX_ROWS - 1,
                col: crate::MAX_COLUMNS - 1,
            }),
        };
        Self {
            store,
            first,
            last,
            forward: None,
            backward: None,
        }
    }
    fn advance(&mut self, reverse: bool) -> Option<(&'a CellAddress, &'a T)> {
        loop {
            let first = self.first?;
            let last = self.last?;
            if first > last {
                return None;
            }
            let cursor = if reverse {
                &mut self.backward
            } else {
                &mut self.forward
            };
            if let Some(current) = cursor {
                if let Some(value) = current.advance(reverse) {
                    if *value.0 < first || *value.0 > last {
                        continue;
                    }
                    if reverse {
                        self.last = before(*value.0);
                    } else {
                        self.first = after(*value.0);
                    }
                    return Some(value);
                }
                let row = current.row;
                *cursor = None;
                if reverse {
                    self.last = if row > 0 {
                        Some(CellAddress {
                            row: row - 1,
                            col: crate::MAX_COLUMNS - 1,
                        })
                    } else {
                        None
                    };
                } else {
                    self.first = if row + 1 < crate::MAX_ROWS {
                        Some(CellAddress {
                            row: row + 1,
                            col: 0,
                        })
                    } else {
                        None
                    };
                }
                continue;
            }
            let mut rows = OccupiedRows::new(self.store, first.row, last.row);
            let row = if reverse {
                rows.next_back()
            } else {
                rows.next()
            }?;
            let first_col = if row == first.row { first.col } else { 0 };
            let last_col = if row == last.row {
                last.col
            } else {
                crate::MAX_COLUMNS - 1
            };
            *cursor = Some(RowCursor::new(
                self.store, row, first_col, last_col, reverse,
            ));
        }
    }
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> Iterator for CellRange<'a, T, HEIGHT, WIDTH> {
    /// Borrowed stored coordinate and payload within the requested interval.
    type Item = (&'a CellAddress, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        self.advance(false)
    }
}
impl<T, const HEIGHT: u32, const WIDTH: u32> DoubleEndedIterator
    for CellRange<'_, T, HEIGHT, WIDTH>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.advance(true)
    }
}

/// Present rows only, independent of empty authored row metadata.
pub struct OccupiedRows<'a> {
    blocks: btree_map::Range<'a, u32, RowBlock>,
    first: u32,
    last: u32,
    forward: Option<(u32, u64)>,
    backward: Option<(u32, u64)>,
}
impl<'a> OccupiedRows<'a> {
    fn new<T, const HEIGHT: u32, const WIDTH: u32>(
        store: &'a CellStore<T, HEIGHT, WIDTH>,
        first: u32,
        last: u32,
    ) -> Self {
        let blocks = if first <= last {
            // SAFETY: Dividing ordered unsigned bounds by the same positive constant preserves their order.
            store
                .occupied_rows
                .range(first / ROW_BLOCK_HEIGHT..=last / ROW_BLOCK_HEIGHT)
        } else {
            // SAFETY: Equal included/excluded bounds form an empty, ordered range.
            store.occupied_rows.range(0..0)
        };
        Self {
            blocks,
            first,
            last,
            forward: None,
            backward: None,
        }
    }
    fn clipped(&self, id: u32, mut mask: u64) -> (u32, u64) {
        if id == self.first / ROW_BLOCK_HEIGHT {
            mask &= u64::MAX << (self.first % ROW_BLOCK_HEIGHT);
        }
        if id == self.last / ROW_BLOCK_HEIGHT {
            mask &= u64::MAX >> (ROW_BLOCK_HEIGHT - 1 - self.last % ROW_BLOCK_HEIGHT);
        }
        (id, mask)
    }
    fn pop(block: &mut Option<(u32, u64)>, reverse: bool) -> Option<u32> {
        let (id, mask) = block.as_mut()?;
        if *mask == 0 {
            *block = None;
            return None;
        }
        let bit = if reverse {
            63 - mask.leading_zeros()
        } else {
            mask.trailing_zeros()
        };
        *mask &= !(1u64 << bit);
        Some(*id * ROW_BLOCK_HEIGHT + bit)
    }
    fn advance(&mut self, reverse: bool) -> Option<u32> {
        loop {
            let active = if reverse {
                &mut self.backward
            } else {
                &mut self.forward
            };
            if let Some(row) = Self::pop(active, reverse) {
                return Some(row);
            }
            let next = if reverse {
                self.blocks.next_back()
            } else {
                self.blocks.next()
            };
            if let Some((id, block)) = next {
                let clipped = self.clipped(*id, block.mask);
                if reverse {
                    self.backward = Some(clipped);
                } else {
                    self.forward = Some(clipped);
                }
            } else {
                // Both ends share the tree range. Once it is empty, consume any
                // remaining bits already borrowed by the opposite direction.
                let opposite = if reverse {
                    &mut self.forward
                } else {
                    &mut self.backward
                };
                return Self::pop(opposite, reverse);
            }
        }
    }
}
impl Iterator for OccupiedRows<'_> {
    /// Index of a row containing at least one stored payload.
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        self.advance(false)
    }
}
impl DoubleEndedIterator for OccupiedRows<'_> {
    fn next_back(&mut self) -> Option<u32> {
        self.advance(true)
    }
}
/// Ordered iterator over stored cells in one column and inclusive row bounds.
/// Absent coordinates are skipped; addresses retain their original worksheet positions.
pub struct ColumnCells<'a, T = Cell, const HEIGHT: u32 = 32, const WIDTH: u32 = 4> {
    store: &'a CellStore<T, HEIGHT, WIDTH>,
    pages: btree_set::Range<'a, (u32, u32)>,
    column: u32,
    next: u32,
    last: u32,
    page: Option<(u32, &'a Page<T, HEIGHT, WIDTH>)>,
}
impl<'a, T, const HEIGHT: u32, const WIDTH: u32> Iterator for ColumnCells<'a, T, HEIGHT, WIDTH> {
    /// Borrowed coordinate and payload from the selected column.
    type Item = (&'a CellAddress, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.next > self.last {
                return None;
            }
            if let Some((block, page)) = self.page {
                let end = ((block + 1) * HEIGHT - 1).min(self.last);
                while self.next <= end {
                    let row = self.next;
                    self.next += 1;
                    if let Some((a, c)) = page
                        .slots
                        .get(
                            (self.column % WIDTH) as usize * HEIGHT as usize
                                + (row % HEIGHT) as usize,
                        )
                        .and_then(Option::as_ref)
                    {
                        return Some((a, c));
                    }
                }
            }
            let (col, row) = *self.pages.next()?;
            self.next = self.next.max(row * HEIGHT);
            self.page = Some((row, self.store.pages.get(&PageId { row, col })?));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellValue;
    #[test]
    fn inverted_row_and_column_ranges_are_empty() {
        let mut cells = CellStore::<Cell>::default();
        cells
            .set(CellAddress { row: 5, col: 1 }, Cell::default())
            .unwrap();
        assert_eq!(cells.cells().occupied_rows(10, 0).next(), None);
        assert_eq!(cells.cells().occupied_rows(10, 0).next_back(), None);
        assert_eq!(cells.cells().column(1, 10, 0).next(), None);
    }
    #[test]
    fn tiles_are_sparse_column_major_and_released_after_last_removal() {
        let mut cells = CellStore::<Cell>::default();
        let addresses = [
            CellAddress { row: 31, col: 3 },
            CellAddress { row: 32, col: 4 },
            CellAddress {
                row: crate::MAX_ROWS - 1,
                col: crate::MAX_COLUMNS - 1,
            },
        ];
        for a in addresses {
            cells.insert(a, Cell::default());
        }
        assert_eq!(cells.pages.len(), 3);
        assert_eq!(cells.cells().len(), 3);
        assert_eq!(offset::<32, 4>(CellAddress { row: 0, col: 1 }), 32);
        assert_eq!(offset::<32, 4>(CellAddress { row: 1, col: 1 }), 33);
        for a in addresses {
            assert_eq!(cells.get(&a).unwrap().value, CellValue::Blank);
            cells.remove(&a).unwrap();
        }
        assert!(cells.pages.is_empty());
        assert!(cells.page_ids.is_empty());
        assert!(cells.column_pages.is_empty());
        assert!(cells.occupied_rows.is_empty());
        let a = CellAddress { row: 0, col: 0 };
        cells.insert(a, Cell::new(1.));
        cells.insert(a, Cell::new(2.));
        assert_eq!(cells.cells().len(), 1);
        assert_eq!(cells.occupied_rows.get(&0).unwrap().counts[0], 1);
        cells.get_mut(&a).unwrap().value = CellValue::Number(3.);
        assert_eq!(cells.get(&a).unwrap().value, CellValue::Number(3.));
    }
    #[test]
    fn row_blocks_preserve_counts_replacement_removal_and_full_row_bound() {
        let mut cells = CellStore::<Cell>::default();
        let row = 63;
        for col in 0..crate::MAX_COLUMNS {
            assert!(
                cells
                    .insert(CellAddress { row, col }, Cell::default())
                    .is_none()
            );
        }
        let block = cells.occupied_rows.get(&0).unwrap();
        assert_eq!(block.counts[63], crate::MAX_COLUMNS as u16);
        assert_eq!(block.mask, 1u64 << 63);
        assert_eq!(cells.cells().occupied_rows(0, 64).collect::<Vec<_>>(), [63]);
        let a = CellAddress { row, col: 0 };
        assert!(cells.insert(a, Cell::new(2.)).is_some());
        assert_eq!(
            cells.occupied_rows.get(&0).unwrap().counts[63],
            crate::MAX_COLUMNS as u16
        );
        // Another row keeps the block alive after the full row is removed.
        let neighbor = CellAddress { row: 0, col: 1 };
        cells.insert(neighbor, Cell::default());
        for col in 0..crate::MAX_COLUMNS {
            assert!(cells.remove(&CellAddress { row, col }).is_some());
        }
        let block = cells.occupied_rows.get(&0).unwrap();
        assert_eq!(block.counts[63], 0);
        assert_eq!(block.mask, 1);
        assert!(cells.remove(&a).is_none());
        assert_eq!(cells.occupied_rows.get(&0).unwrap().counts[0], 1);
        cells.remove(&neighbor).unwrap();
        assert!(cells.occupied_rows.is_empty());
        cells.insert(a, Cell::default());
        assert_eq!(cells.occupied_rows.get(&0).unwrap().counts[63], 1);
        assert_eq!(
            cells.cells().occupied_rows(63, 63).collect::<Vec<_>>(),
            [63]
        );
    }
    #[test]
    fn row_blocks_clip_boundaries_and_support_reverse_and_mixed_consumption() {
        let mut cells = CellStore::<Cell>::default();
        let mut reference = BTreeMap::new();
        for row in [0, 1, 31, 62, 63, 64, 65, 127, 128, crate::MAX_ROWS - 1] {
            for col in [0, 3, crate::MAX_COLUMNS - 1] {
                let a = CellAddress { row, col };
                let cell = Cell::default();
                cells.insert(a, cell.clone());
                reference.insert(a, cell);
            }
        }
        assert_eq!(cells.occupied_rows.len(), 4);
        for (first, last) in [
            (0, crate::MAX_ROWS - 1),
            (0, 0),
            (63, 63),
            (64, 64),
            (1, 63),
            (63, 64),
            (62, 127),
            (66, 126),
            (129, crate::MAX_ROWS - 2),
            (crate::MAX_ROWS - 1, crate::MAX_ROWS - 1),
        ] {
            let expected: Vec<_> = reference
                .keys()
                .map(|a| a.row)
                .filter(|row| (first..=last).contains(row))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            assert_eq!(
                cells.cells().occupied_rows(first, last).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                cells
                    .cells()
                    .occupied_rows(first, last)
                    .rev()
                    .collect::<Vec<_>>(),
                expected.iter().rev().copied().collect::<Vec<_>>()
            );
            // Exercise each possible switch point, both with shared and distinct blocks.
            for split in 0..=expected.len() {
                let mut actual = cells.cells().occupied_rows(first, last);
                let mut expected = std::collections::VecDeque::from(expected.clone());
                for _ in 0..split {
                    assert_eq!(actual.next(), expected.pop_front());
                }
                while !expected.is_empty() {
                    assert_eq!(actual.next_back(), expected.pop_back());
                    assert_eq!(actual.next(), expected.pop_front());
                }
                assert_eq!(actual.next(), None);
                assert_eq!(actual.next_back(), None);
            }
            let low = CellAddress { row: first, col: 2 };
            let high = CellAddress {
                row: last,
                col: crate::MAX_COLUMNS - 2,
            };
            if low > high {
                continue;
            }
            let mut actual = cells.cells().range(low..=high);
            let mut expected = reference.range(low..=high);
            loop {
                let back = expected.next_back();
                assert_eq!(actual.next_back(), back);
                if back.is_none() {
                    break;
                }
                let front = expected.next();
                assert_eq!(actual.next(), front);
                if front.is_none() {
                    break;
                }
            }
            assert_eq!(actual.next(), None);
            assert_eq!(actual.next_back(), None);
        }
    }
    #[test]
    fn ordered_ranges_and_mixed_direction_match_coordinate_tree() {
        let mut cells = CellStore::<Cell>::default();
        let mut reference = BTreeMap::new();
        let mut state = 13u64;
        for _ in 0..2000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let a = CellAddress {
                row: (state >> 32) as u32 % 103,
                col: (state >> 16) as u32 % 19,
            };
            let cell = Cell::new(a.row as f64 * 100. + a.col as f64);
            cells.insert(a, cell.clone());
            reference.insert(a, cell);
        }
        assert_eq!(
            cells.cells().keys().copied().collect::<Vec<_>>(),
            reference.keys().copied().collect::<Vec<_>>()
        );
        for (first, last) in [
            (
                CellAddress { row: 0, col: 0 },
                CellAddress { row: 102, col: 18 },
            ),
            (
                CellAddress { row: 31, col: 3 },
                CellAddress { row: 97, col: 4 },
            ),
            (
                CellAddress { row: 32, col: 4 },
                CellAddress { row: 32, col: 5 },
            ),
        ] {
            let expected: Vec<_> = reference.range(first..=last).collect();
            assert_eq!(
                cells.cells().range(first..=last).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                cells.cells().range(first..=last).rev().collect::<Vec<_>>(),
                expected.iter().rev().copied().collect::<Vec<_>>()
            );
            let mut actual = cells.cells().range(first..=last);
            let mut expected = reference.range(first..=last);
            loop {
                let next = expected.next();
                assert_eq!(actual.next(), next);
                if next.is_none() {
                    break;
                }
                let back = expected.next_back();
                assert_eq!(actual.next_back(), back);
                if back.is_none() {
                    break;
                }
            }
        }
        let a = CellAddress { row: 32, col: 4 };
        let b = CellAddress { row: 96, col: 8 };
        assert_eq!(
            cells
                .cells()
                .range((Bound::Excluded(a), Bound::Excluded(b)))
                .collect::<Vec<_>>(),
            reference
                .range((Bound::Excluded(a), Bound::Excluded(b)))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn point_mutations_and_clone_match_coordinate_tree() {
        let mut cells = CellStore::<Cell>::default();
        let mut reference = BTreeMap::new();
        let mut state = 29u64;
        for i in 0..2000 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let a = CellAddress {
                row: (state >> 32) as u32 % 103,
                col: (state >> 16) as u32 % 19,
            };
            if i % 3 == 0 {
                assert_eq!(cells.remove(&a), reference.remove(&a));
            } else {
                let cell = if i % 5 == 0 {
                    Cell::default()
                } else {
                    Cell::new(i as f64)
                };
                assert_eq!(cells.insert(a, cell.clone()), reference.insert(a, cell));
            }
            assert_eq!(cells.get(&a), reference.get(&a));
            if let Some(cell) = cells.get_mut(&a) {
                cell.style.number_format = Some("0.00".into());
                reference.get_mut(&a).unwrap().style.number_format = Some("0.00".into());
            }
        }
        assert_eq!(
            cells.cells().iter().collect::<Vec<_>>(),
            reference.iter().collect::<Vec<_>>()
        );
        let cloned = cells.clone();
        let mut reversed = CellStore::<Cell>::default();
        for (a, cell) in reference.iter().rev() {
            reversed.insert(*a, cell.clone());
        }
        assert_eq!(cells, cloned);
        assert_eq!(cells, reversed);
        for a in reference.keys() {
            assert_eq!(cells.remove(a), reference.get(a).cloned());
        }
        assert!(cells.cells().is_empty());
        assert!(cells.pages.is_empty());
        assert!(cells.page_ids.is_empty());
        assert!(cells.column_pages.is_empty());
        assert!(cells.occupied_rows.is_empty());
        assert_eq!(cloned.cells().len(), reference.len());
        assert_ne!(cells, cloned);
        // Recreate a removed tile with an explicit blank and an absent neighbor.
        let a = CellAddress { row: 32, col: 4 };
        cells.insert(a, Cell::default());
        assert_eq!(cells.get(&a).unwrap().value, CellValue::Blank);
        assert!(cells.get(&CellAddress { row: 32, col: 5 }).is_none());
        assert_eq!(cells.cells().iter().count(), 1);
    }
    #[test]
    fn column_scans_clip_tiles_skip_absent_slots_and_keep_stored_blanks() {
        let mut cells = CellStore::<Cell>::default();
        let mut reference = BTreeMap::new();
        for row in [0, 31, 32, 33, 63, 64, 95, 96, crate::MAX_ROWS - 1] {
            for col in [0, 3, 4, 8] {
                let a = CellAddress { row, col };
                let cell = if col == 3 {
                    Cell::default()
                } else {
                    Cell::new(row as f64)
                };
                cells.insert(a, cell.clone());
                reference.insert(a, cell);
            }
        }
        for col in 0..10 {
            for (first, last) in [(0, crate::MAX_ROWS - 1), (31, 64), (33, 63), (65, 94)] {
                let expected: Vec<_> = reference
                    .iter()
                    .filter(|(a, _)| a.col == col && (first..=last).contains(&a.row))
                    .collect();
                assert_eq!(
                    cells.cells().column(col, first, last).collect::<Vec<_>>(),
                    expected
                );
            }
        }
        assert!(cells.pages.len() < 30);
    }
}

#[cfg(test)]
mod generic_layout_tests {
    use super::*;
    /// Non-cell test payload proving that storage geometry is reusable.
    #[derive(Debug, PartialEq)]
    struct Payload(u64);
    fn verify<const H: u32, const W: u32>() {
        let mut grid = CellStore::<Payload, H, W>::default();
        let addresses = [
            CellAddress { row: 0, col: 0 },
            CellAddress { row: 15, col: 1 },
            CellAddress { row: 16, col: 2 },
            CellAddress { row: 63, col: 3 },
            CellAddress { row: 64, col: 4 },
            CellAddress {
                row: crate::MAX_ROWS - 1,
                col: crate::MAX_COLUMNS - 1,
            },
        ];
        for (i, at) in addresses.iter().enumerate() {
            grid.set(*at, Payload(i as u64)).unwrap();
        }
        assert_eq!(grid.cells().keys().copied().collect::<Vec<_>>(), addresses);
        assert_eq!(
            grid.cells()
                .iter()
                .rev()
                .map(|(at, _)| *at)
                .collect::<Vec<_>>(),
            addresses.into_iter().rev().collect::<Vec<_>>()
        );
        assert_eq!(
            grid.cells().column(3, 0, 100).next().unwrap().1,
            &Payload(3)
        );
        assert_eq!(
            crate::CellRead::get_cell(&grid, addresses[2]),
            Some(&Payload(2))
        );
        assert!(
            grid.set(
                CellAddress {
                    row: crate::MAX_ROWS,
                    col: 0
                },
                Payload(9)
            )
            .is_err()
        );
        grid.remove(&addresses[2]);
        assert_eq!(grid.cells().len(), 5);
    }
    #[test]
    fn arbitrary_payloads_and_layouts_share_the_same_queries() {
        verify::<16, 2>();
        verify::<16, 4>();
        verify::<32, 4>();
        verify::<31, 3>();
    }
}
