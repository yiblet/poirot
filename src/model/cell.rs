//! Sparse read access to workbook storage.
use crate::model::{Cell, CellAddress, CellStore, Worksheet};
pub(in crate::model) use poirot_io::cell::normalize_formula;
/// Common sparse read access for worksheets and paged stores with arbitrary payloads.
///
/// This abstraction exposes stored entries rather than a dense grid of blank values.
/// It does not evaluate formulas or overlay scenario edits on worksheet content.
pub trait CellRead<T> {
    /// Borrow a stored cell without materializing absent blanks.
    fn get_cell(&self, address: CellAddress) -> Option<&T>;
    /// Iterate explicitly stored cells in coordinate order.
    fn stored_cells<'a>(&'a self) -> impl DoubleEndedIterator<Item = (&'a CellAddress, &'a T)>
    where
        T: 'a;
}
impl<T, const H: u32, const W: u32> CellRead<T> for CellStore<T, H, W> {
    fn get_cell(&self, address: CellAddress) -> Option<&T> {
        self.get(&address)
    }
    fn stored_cells<'a>(&'a self) -> impl DoubleEndedIterator<Item = (&'a CellAddress, &'a T)>
    where
        T: 'a,
    {
        self.cells().iter()
    }
}
impl CellRead<Cell> for Worksheet {
    fn get_cell(&self, address: CellAddress) -> Option<&Cell> {
        self.get(address)
    }
    fn stored_cells<'a>(&'a self) -> impl DoubleEndedIterator<Item = (&'a CellAddress, &'a Cell)>
    where
        Cell: 'a,
    {
        self.cells().iter()
    }
}
