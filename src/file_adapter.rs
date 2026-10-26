//! Direct file-record adaptation over sparse workbook storage.
use crate::model::{
    Cell, CellAddress, CellValue, DateSystem, FileMetadata, ModelError, NamedRange, RowVisibility,
    SheetPresentation, StyleContext, Workbook, Worksheet,
};
use poirot_io::data::{SheetSink, SheetSource, WorkbookSink, WorkbookSource};

impl SheetSource for Worksheet {
    fn name(&self) -> &str {
        self.name()
    }
    fn cells(&self) -> impl DoubleEndedIterator<Item = (&CellAddress, &Cell)> + ExactSizeIterator {
        self.cells().iter()
    }
    fn get(&self, address: CellAddress) -> Option<&Cell> {
        self.get(address)
    }
    fn row_indices(&self) -> impl DoubleEndedIterator<Item = u32> + ExactSizeIterator {
        self.row_indices()
    }
    fn row_visibility(&self, row: u32) -> RowVisibility {
        self.row_visibility(row)
    }
    fn presentation(&self) -> SheetPresentation {
        self.presentation()
    }
    fn spill_extents(&self) -> impl Iterator<Item = (CellAddress, CellAddress)> {
        self.spill_extents()
    }
    fn spill_extent(&self, anchor: CellAddress) -> Option<CellAddress> {
        self.spill_extent(anchor)
    }
    fn spill_owner(&self, address: CellAddress) -> Option<CellAddress> {
        self.spill_owner(address)
    }
    fn stored_range(
        &self,
        first: CellAddress,
        last: CellAddress,
    ) -> impl Iterator<Item = (&CellAddress, &Cell)> {
        self.cells()
            .range(first..=last)
            .filter(move |(at, _)| at.col >= first.col && at.col <= last.col)
    }
}
impl SheetSink for Worksheet {
    fn get_mut(&mut self, address: CellAddress) -> Option<&mut Cell> {
        self.get_mut(address)
    }
    fn set(&mut self, address: CellAddress, cell: Cell) -> Result<(), ModelError> {
        self.set(address, cell).map(|_| ())
    }
    fn create_row(&mut self, row: u32) -> Result<(), ModelError> {
        self.create_row(row).map(|_| ())
    }
    fn set_row_visibility(
        &mut self,
        row: u32,
        visibility: RowVisibility,
    ) -> Result<(), ModelError> {
        self.set_row_visibility(row, visibility)
    }
    fn set_presentation(&mut self, presentation: SheetPresentation) {
        self.set_presentation(presentation);
    }
    fn set_formula_with_cached_value(
        &mut self,
        address: CellAddress,
        formula: String,
        value: CellValue,
    ) -> Result<(), ModelError> {
        self.set_formula_with_cached_value(address, formula, value)
    }
    fn set_spill_extent(
        &mut self,
        anchor: CellAddress,
        last: CellAddress,
        formula: &str,
    ) -> Result<(), ModelError> {
        self.set_spill_extent(anchor, last, formula)
    }
    fn set_spill_cached_value(
        &mut self,
        address: CellAddress,
        anchor: CellAddress,
        formula: &str,
        value: CellValue,
    ) -> Result<(), ModelError> {
        self.set_spill_cached_value(address, anchor, formula, value)
    }
}
impl WorkbookSource for Workbook {
    type Sheet = Worksheet;
    fn sheets(&self) -> &[Worksheet] {
        self.sheets()
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
        self.import_sheet(name)
    }
    fn sheet_mut(&mut self, index: usize) -> Option<&mut Worksheet> {
        self.sheet_mut(index)
    }
    fn sheet_by_name_mut(&mut self, name: &str) -> Option<&mut Worksheet> {
        self.sheet_by_name_mut(name)
    }
    fn set_date_system(&mut self, date_system: DateSystem) {
        self.date_system = date_system;
    }
    fn named_ranges_mut(&mut self) -> &mut Vec<NamedRange> {
        &mut self.named_ranges
    }
    fn set_style_context(&mut self, styles: StyleContext) {
        self.style_context = styles;
    }
    fn set_file_metadata(&mut self, metadata: FileMetadata) {
        self.file_metadata = Some(metadata);
    }
    fn replace_cells(
        &mut self,
        cells: Vec<(usize, CellAddress, Option<Cell>)>,
    ) -> Result<(), ModelError> {
        self.replace_cells(cells)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_source_view_skips_neighbor_columns_and_keeps_stored_blanks() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Data").unwrap();
        for address in ["A1", "B1", "C1", "A2", "B2", "C2", "A3", "B3", "C3"] {
            sheet
                .set_value(address.parse().unwrap(), CellValue::Blank)
                .unwrap();
        }
        let selected =
            SheetSource::stored_range(sheet, "B1".parse().unwrap(), "B3".parse().unwrap())
                .map(|(at, cell)| (at.to_string(), cell.value.clone()))
                .collect::<Vec<_>>();
        assert_eq!(
            selected,
            [
                ("B1".into(), CellValue::Blank),
                ("B2".into(), CellValue::Blank),
                ("B3".into(), CellValue::Blank)
            ]
        );
    }

    #[test]
    fn direct_sink_retains_sparse_rows_styles_and_array_ownership() {
        let mut book = Workbook::default();
        let sheet = WorkbookSink::import_sheet(&mut book, "Data").unwrap();
        SheetSink::create_row(sheet, 400).unwrap();
        SheetSink::set_row_visibility(
            sheet,
            400,
            RowVisibility {
                manually_hidden: true,
                filtered_out: false,
            },
        )
        .unwrap();
        let anchor = "A1".parse().unwrap();
        let child = "B1".parse().unwrap();
        let formula = "{1,2}";
        SheetSink::set(
            sheet,
            anchor,
            Cell::with_formula(formula, CellValue::Number(1.)).unwrap(),
        )
        .unwrap();
        SheetSink::set_spill_extent(sheet, anchor, child, formula).unwrap();
        SheetSink::set_spill_cached_value(sheet, child, anchor, formula, CellValue::Number(2.))
            .unwrap();
        let mut blank = Cell::default();
        blank.style.number_format = Some("0.00".into());
        SheetSink::set(sheet, "C1".parse().unwrap(), blank.clone()).unwrap();
        assert_eq!(
            SheetSource::row_indices(sheet).collect::<Vec<_>>(),
            [0, 400]
        );
        assert!(SheetSource::row_visibility(sheet, 400).manually_hidden);
        assert_eq!(SheetSource::get(sheet, "C1".parse().unwrap()), Some(&blank));
        assert_eq!(SheetSource::spill_owner(sheet, child), Some(anchor));
        assert_eq!(
            SheetSource::spill_extents(sheet).collect::<Vec<_>>(),
            [(anchor, child)]
        );
        assert_eq!(SheetSource::cells(sheet).len(), 3);
    }
}
