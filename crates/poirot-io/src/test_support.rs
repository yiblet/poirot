//! Small in-memory file-record sink used only to test codecs independently of Poirot.
use crate::{data::*, *};
use std::collections::{BTreeMap, BTreeSet};

/// Sparse worksheet fixture for file codecs, with explicit saved array ownership.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Worksheet {
    name: String,
    cells: BTreeMap<CellAddress, Cell>,
    // Deliberately accessible so codec tests can supply invalid custom-source rows.
    pub(super) rows: BTreeSet<u32>,
    visibility: BTreeMap<u32, RowVisibility>,
    presentation: SheetPresentation,
    extents: BTreeMap<CellAddress, (CellAddress, String)>,
    children: BTreeMap<CellAddress, (CellAddress, Option<Cell>)>,
}
impl Worksheet {
    /// Create empty file-record fixture storage.
    pub fn new(name: impl Into<String>) -> Result<Self, ModelError> {
        Ok(Self {
            name: name.into(),
            ..Self::default()
        })
    }
    /// Store a literal fixture value.
    pub fn set_value(
        &mut self,
        at: CellAddress,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        self.set(at, Cell::new(value))
    }
    /// Store a fixture formula and saved value.
    pub fn set_formula_with_cached_value(
        &mut self,
        at: CellAddress,
        formula: impl Into<String>,
        value: CellValue,
    ) -> Result<(), ModelError> {
        self.set(at, Cell::with_formula(formula, value)?)
    }
    /// Borrow authored content underneath saved array caches.
    pub fn source_cell(&self, at: CellAddress) -> Option<&Cell> {
        self.children
            .get(&at)
            .map_or_else(|| self.cells.get(&at), |(_, cell)| cell.as_ref())
    }
    /// Identify a fully populated saved array rectangle.
    pub fn complete_spill_extent(&self, at: CellAddress) -> Option<CellAddress> {
        let last = self.spill_extent(at)?;
        let size = u64::from(last.row - at.row + 1) * u64::from(last.col - at.col + 1);
        (self
            .children
            .values()
            .filter(|(owner, _)| *owner == at)
            .count() as u64
            + 1
            == size)
            .then_some(last)
    }
}
impl SheetSource for Worksheet {
    fn name(&self) -> &str {
        &self.name
    }
    fn cells(&self) -> impl DoubleEndedIterator<Item = (&CellAddress, &Cell)> + ExactSizeIterator {
        self.cells.iter()
    }
    fn get(&self, at: CellAddress) -> Option<&Cell> {
        self.cells.get(&at)
    }
    fn row_indices(&self) -> impl DoubleEndedIterator<Item = u32> + ExactSizeIterator {
        self.rows.iter().copied()
    }
    fn row_visibility(&self, at: u32) -> RowVisibility {
        self.visibility.get(&at).copied().unwrap_or_default()
    }
    fn presentation(&self) -> SheetPresentation {
        self.presentation
    }
    fn spill_extents(&self) -> impl Iterator<Item = (CellAddress, CellAddress)> {
        self.extents.iter().filter_map(|(at, (last, formula))| {
            (self.get(*at)?.formula.as_ref()? == formula).then_some((*at, *last))
        })
    }
    fn spill_extent(&self, at: CellAddress) -> Option<CellAddress> {
        self.spill_extents()
            .find_map(|(first, last)| (first == at).then_some(last))
    }
    fn spill_owner(&self, at: CellAddress) -> Option<CellAddress> {
        self.children.get(&at).map(|(owner, _)| *owner)
    }
    fn stored_range(
        &self,
        first: CellAddress,
        last: CellAddress,
    ) -> impl Iterator<Item = (&CellAddress, &Cell)> {
        self.cells
            .range(first..=last)
            .filter(move |(at, _)| at.col >= first.col && at.col <= last.col)
    }
}
impl SheetSink for Worksheet {
    fn get_mut(&mut self, at: CellAddress) -> Option<&mut Cell> {
        self.cells.get_mut(&at)
    }
    fn set(&mut self, at: CellAddress, cell: Cell) -> Result<(), ModelError> {
        at.validate()?;
        cell.validate_shared_formula()?;
        self.extents.remove(&at);
        self.children.retain(|_, (owner, _)| *owner != at);
        self.rows.insert(at.row);
        self.cells.insert(at, cell);
        Ok(())
    }
    fn create_row(&mut self, row: u32) -> Result<(), ModelError> {
        CellAddress::new(row, 0)?;
        self.rows.insert(row);
        Ok(())
    }
    fn set_row_visibility(&mut self, row: u32, v: RowVisibility) -> Result<(), ModelError> {
        self.create_row(row)?;
        self.visibility.insert(row, v);
        Ok(())
    }
    fn set_presentation(&mut self, v: SheetPresentation) {
        self.presentation = v;
    }
    fn set_formula_with_cached_value(
        &mut self,
        at: CellAddress,
        formula: String,
        v: CellValue,
    ) -> Result<(), ModelError> {
        self.set_formula_with_cached_value(at, formula, v)
    }
    fn set_spill_extent(
        &mut self,
        at: CellAddress,
        last: CellAddress,
        formula: &str,
    ) -> Result<(), ModelError> {
        at.validate()?;
        last.validate()?;
        self.extents.insert(at, (last, formula.into()));
        Ok(())
    }
    fn set_spill_cached_value(
        &mut self,
        at: CellAddress,
        owner: CellAddress,
        _formula: &str,
        value: CellValue,
    ) -> Result<(), ModelError> {
        let source = self.cells.remove(&at);
        let style = source.as_ref().map(|c| c.style.clone()).unwrap_or_default();
        self.children.insert(at, (owner, source));
        self.rows.insert(at.row);
        self.cells.insert(
            at,
            Cell {
                value,
                style,
                ..Cell::default()
            },
        );
        Ok(())
    }
}
/// Minimal owned workbook fixture for testing direct file-record construction.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Workbook {
    pub date_system: DateSystem,
    pub named_ranges: Vec<NamedRange>,
    pub file_metadata: Option<FileMetadata>,
    pub style_context: StyleContext,
    sheets: Vec<Worksheet>,
}
impl Workbook {
    /// Create empty file-record fixture storage.
    pub fn new() -> Self {
        Self::default()
    }
    /// Append a named fixture sheet.
    pub fn add_sheet(&mut self, name: impl Into<String>) -> Result<&mut Worksheet, ModelError> {
        self.import_sheet(&name.into())
    }
    /// Borrow a fixture sheet by persisted position.
    pub fn sheet(&self, index: usize) -> Option<&Worksheet> {
        self.sheets.get(index)
    }
}
impl WorkbookSource for Workbook {
    type Sheet = Worksheet;
    fn sheets(&self) -> &[Worksheet] {
        &self.sheets
    }
    fn date_system(&self) -> DateSystem {
        self.date_system
    }
    fn named_ranges(&self) -> &[NamedRange] {
        &self.named_ranges
    }
    fn style_context(&self) -> &StyleContext {
        &self.style_context
    }
}
impl WorkbookSink for Workbook {
    fn import_sheet(&mut self, name: &str) -> Result<&mut Worksheet, ModelError> {
        self.sheets.push(Worksheet::new(name)?);
        self.sheets
            .last_mut()
            .ok_or(ModelError::MissingFixtureSheet)
    }
    fn sheet_mut(&mut self, index: usize) -> Option<&mut Worksheet> {
        self.sheets.get_mut(index)
    }
    fn sheet_by_name_mut(&mut self, name: &str) -> Option<&mut Worksheet> {
        self.sheets.iter_mut().find(|s| s.name() == name)
    }
    fn set_date_system(&mut self, v: DateSystem) {
        self.date_system = v;
    }
    fn named_ranges_mut(&mut self) -> &mut Vec<NamedRange> {
        &mut self.named_ranges
    }
    fn set_style_context(&mut self, v: StyleContext) {
        self.style_context = v;
    }
    fn set_file_metadata(&mut self, v: FileMetadata) {
        self.file_metadata = Some(v);
    }
    fn replace_cells(
        &mut self,
        cells: Vec<(usize, CellAddress, Option<Cell>)>,
    ) -> Result<(), ModelError> {
        for (sheet, at, value) in cells {
            let sheet = self
                .sheet_mut(sheet)
                .ok_or(ModelError::MissingFixtureSheet)?;
            match value {
                Some(value) => {
                    sheet.cells.insert(at, value);
                }
                None => {
                    sheet.cells.remove(&at);
                }
            }
        }
        Ok(())
    }
}
