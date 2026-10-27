//! Borrowed source and streaming import contracts, independent of workbook storage.
use crate::{
    Cell, CellAddress, CellValue, DateSystem, FileMetadata, ModelError, NamedRange, RowVisibility,
    SheetPresentation, StyleContext,
};

/// Sparse worksheet data consumed by Excel encoders.
/// Addresses and row indices are zero-based within Excel's grid. Implementations
/// expose saved values and formula caches; encoders do not calculate formulas.
/// Explicit blank cells and rows remain distinct from absent storage.
pub trait SheetSource {
    /// Borrow the stored worksheet name.
    fn name(&self) -> &str;
    /// Iterate saved source cells in coordinate order, including spill caches.
    fn cells(&self) -> impl DoubleEndedIterator<Item = (&CellAddress, &Cell)> + ExactSizeIterator;
    /// Borrow a saved source cell without materializing blanks.
    fn get(&self, address: CellAddress) -> Option<&Cell>;
    /// Iterate unique, valid Excel row indices in increasing order.
    fn row_indices(&self) -> impl DoubleEndedIterator<Item = u32> + ExactSizeIterator;
    /// Read persisted row hiding and filtering flags.
    fn row_visibility(&self, row: u32) -> RowVisibility;
    /// Read persisted worksheet presentation.
    fn presentation(&self) -> SheetPresentation;
    /// Iterate declared saved array extents without expanding their coordinates.
    fn spill_extents(&self) -> impl Iterator<Item = (CellAddress, CellAddress)>;
    /// Read a saved array's final coordinate.
    fn spill_extent(&self, anchor: CellAddress) -> Option<CellAddress>;
    /// Read the saved array anchor owning this cell.
    fn spill_owner(&self, address: CellAddress) -> Option<CellAddress>;
    /// Iterate only stored cells within an inclusive rectangular range.
    /// Callers supply valid, ordered endpoints; absent cells must not be materialized.
    fn stored_range(
        &self,
        first: CellAddress,
        last: CellAddress,
    ) -> impl Iterator<Item = (&CellAddress, &Cell)>;
}

/// Worksheet mutations needed to attach decoded file records directly to storage.
pub trait SheetSink: SheetSource {
    /// Borrow a decoded cell for a fidelity overlay.
    fn get_mut(&mut self, address: CellAddress) -> Option<&mut Cell>;
    /// Store one decoded cell, validating coordinates and provenance.
    fn set(&mut self, address: CellAddress, cell: Cell) -> Result<(), ModelError>;
    /// Retain an explicit row without allocating its absent cells.
    fn create_row(&mut self, row: u32) -> Result<(), ModelError>;
    /// Attach persisted row hiding and filtering flags.
    fn set_row_visibility(&mut self, row: u32, visibility: RowVisibility)
    -> Result<(), ModelError>;
    /// Attach persisted worksheet presentation.
    fn set_presentation(&mut self, presentation: SheetPresentation);
    /// Attach a recovered formula and its saved value.
    fn set_formula_with_cached_value(
        &mut self,
        address: CellAddress,
        formula: String,
        value: CellValue,
    ) -> Result<(), ModelError>;
    /// Attach a checked saved array extent to its source formula.
    fn set_spill_extent(
        &mut self,
        anchor: CellAddress,
        last: CellAddress,
        formula: &str,
    ) -> Result<(), ModelError>;
    /// Attach one stored saved array child without expanding its rectangle.
    fn set_spill_cached_value(
        &mut self,
        address: CellAddress,
        anchor: CellAddress,
        formula: &str,
        value: CellValue,
    ) -> Result<(), ModelError>;
}

/// Borrowed workbook records consumed by Excel encoders without cloning source state.
pub trait WorkbookSource {
    /// Storage's borrowed worksheet representation.
    type Sheet: SheetSource;
    /// Borrow sheets in their persisted order.
    fn sheets(&self) -> &[Self::Sheet];
    /// Read the calendar associated with saved numeric dates.
    fn date_system(&self) -> DateSystem;
    /// Borrow saved names and formulas.
    fn named_ranges(&self) -> &[NamedRange];
    /// Borrow workbook theme and indexed colors.
    fn style_context(&self) -> &StyleContext;
}

/// Direct construction and fidelity recovery of decoded workbook records.
/// Codecs construct a private sink and return it only after successful import;
/// individual sink methods need not provide transaction rollback for callers.
/// `Default` must create empty storage; codecs append decoded sheets and records.
pub trait WorkbookSink: WorkbookSource + Default
where
    Self::Sheet: SheetSink,
{
    /// Add a decoded sheet while preserving source spelling.
    fn import_sheet(&mut self, name: &str) -> Result<&mut Self::Sheet, ModelError>;
    /// Borrow a decoded sheet by persisted position for recovery.
    fn sheet_mut(&mut self, index: usize) -> Option<&mut Self::Sheet>;
    /// Borrow a decoded sheet by source name using the storage identity policy for recovery.
    fn sheet_by_name_mut(&mut self, name: &str) -> Option<&mut Self::Sheet>;
    /// Attach the source calendar.
    fn set_date_system(&mut self, date_system: DateSystem);
    /// Attach decoded saved names.
    fn named_ranges_mut(&mut self) -> &mut Vec<NamedRange>;
    /// Attach decoded theme and indexed colors.
    fn set_style_context(&mut self, styles: StyleContext);
    /// Attach source format and import diagnostics.
    fn set_file_metadata(&mut self, metadata: FileMetadata);
    /// Replace decoded scalar sources in one sparse batch before publishing array ownership.
    fn replace_cells(
        &mut self,
        cells: Vec<(usize, CellAddress, Option<Cell>)>,
    ) -> Result<(), ModelError>;
}
