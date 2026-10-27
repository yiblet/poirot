//! Checked retained payload/count accounting for callers before model cloning.
//! Counts describe model storage, not allocator overhead or resident memory.
//!
//! ```rust
//! use poirot::model::{CellValue, Workbook};
//! use poirot::model::clone_footprint::CloneFootprint;
//!
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_value("A1", "baseline")?;
//! let before = CloneFootprint::measure(&book).ok_or("accounting overflow")?;
//! let projected = CloneFootprint::measure_with_assignments(
//!     &book, &[(0, "B1".parse()?, CellValue::Number(42.))],
//! ).ok_or("invalid assignment or accounting overflow")?;
//! assert_eq!(projected.cells, before.cells + 1);
//! assert_eq!(book.sheet(0).ok_or("missing sheet")?.cells().len(), 1);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::model::Workbook;

/// Logical record counts and text bytes that a workbook clone would retain.
/// See the [module example](crate::model::clone_footprint) for checking projected assignments without cloning.
/// Used to check caller limits before cloning; excludes allocator overhead and peak memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CloneFootprint {
    /// Worksheet records included in a workbook clone.
    pub sheets: usize,
    /// Stored cell payloads included in a workbook clone.
    pub cells: usize,
    /// Explicit row-presence records included in a workbook clone.
    pub rows: usize,
    /// Explicit visibility overrides included in a workbook clone.
    pub row_visibility: usize,
    /// Defined-name records included in a workbook clone.
    pub named_ranges: usize,
    /// Every logical textual payload retained by Workbook: names, formulas, formats
    /// and font names (counted per cell), plus theme/palette metadata bytes
    /// and cell text (including physical UTF-16 bytes).
    pub text_bytes: usize,
}
impl CloneFootprint {
    /// Exact storage accounting for resolved set_value assignments, without a clone.
    /// Later assignments to a coordinate replace earlier ones.
    pub fn measure_with_assignments(
        workbook: &Workbook,
        assignments: &[(usize, crate::model::CellAddress, crate::model::CellValue)],
    ) -> Option<Self> {
        let mut result = Self::measure(workbook)?;
        let mut final_values = std::collections::BTreeMap::new();
        for (sheet, address, value) in assignments {
            address.validate().ok()?;
            workbook.sheet(*sheet)?;
            final_values.insert((*sheet, *address), value);
        }
        let mut invalidated_extents = std::collections::BTreeSet::new();
        for &(sheet, address) in final_values.keys() {
            let worksheet = workbook.sheet(sheet)?;
            if worksheet.spill_extents.contains_key(&address) {
                invalidated_extents.insert((sheet, address));
            }
            if let Some(child) = worksheet.spill_cache.get(&address) {
                invalidated_extents.insert((sheet, child.owner));
            }
        }
        for (sheet, anchor) in invalidated_extents {
            if let Some(extent) = workbook.sheet(sheet)?.spill_extents.get(&anchor) {
                result.text_bytes = result.text_bytes.checked_sub(extent.formula.len())?;
            }
        }
        for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
            for (address, spill) in &worksheet.spill_cache {
                let removed_style = if final_values.contains_key(&(sheet, spill.owner)) {
                    worksheet.get_validated(*address).map(|cell| &cell.style)
                } else if final_values.contains_key(&(sheet, *address)) {
                    spill.source.as_ref().map(|cell| &cell.style)
                } else {
                    None
                };
                if let Some(style) = removed_style {
                    result.text_bytes = result.text_bytes.checked_sub(style.text_bytes()?)?;
                }
            }
        }
        let mut added_rows = std::collections::BTreeSet::new();
        for ((sheet, address), value) in final_values {
            let worksheet = workbook.sheet(sheet)?;
            if let Some(cell) = worksheet.get_validated(address) {
                result.text_bytes = result
                    .text_bytes
                    .checked_sub(cell.value.text_storage_bytes())?;
                result.text_bytes = result
                    .text_bytes
                    .checked_sub(cell.formula.as_ref().map_or(0, String::len))?;
            } else {
                result.cells = result.cells.checked_add(1)?;
                if !worksheet.rows.contains(&address.row) && added_rows.insert((sheet, address.row))
                {
                    result.rows = result.rows.checked_add(1)?;
                }
            }
            result.text_bytes = result.text_bytes.checked_add(value.text_storage_bytes())?;
        }
        Some(result)
    }
    /// Count retained source records without cloning their payloads.
    pub fn measure(workbook: &Workbook) -> Option<Self> {
        let mut result = Self {
            sheets: workbook.sheets.len(),
            named_ranges: workbook.named_ranges.len(),
            text_bytes: workbook.style_context.retained_bytes()?,
            ..Self::default()
        };
        for sheet in &workbook.sheets {
            result.cells = result.cells.checked_add(sheet.cells().len())?;
            result.rows = result.rows.checked_add(sheet.rows.len())?;
            result.row_visibility = result.row_visibility.checked_add(sheet.visibility.len())?;
            result.text_bytes = result.text_bytes.checked_add(sheet.name.len())?;
            for extent in sheet.spill_extents.values() {
                result.text_bytes = result.text_bytes.checked_add(extent.formula.len())?;
            }
            for cell in sheet
                .spill_cache
                .values()
                .filter_map(|spill| spill.source.as_ref())
            {
                result.text_bytes = result.text_bytes.checked_add(cell.style.text_bytes()?)?;
            }
            for cell in sheet.cells().values() {
                result.text_bytes = result
                    .text_bytes
                    .checked_add(cell.value.text_storage_bytes())?;
                result.text_bytes = result
                    .text_bytes
                    .checked_add(cell.formula.as_ref().map_or(0, String::len))?;
                result.text_bytes = result.text_bytes.checked_add(cell.style.text_bytes()?)?;
            }
        }
        for name in &workbook.named_ranges {
            result.text_bytes = result.text_bytes.checked_add(name.name.len())?;
            result.text_bytes = result.text_bytes.checked_add(name.formula.len())?;
        }
        if let Some(metadata) = &workbook.file_metadata {
            for diagnostic in &metadata.import_warnings {
                result.text_bytes = result.text_bytes.checked_add(diagnostic.detail.len())?;
            }
        }
        Some(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CellValue, ExcelText, NamedRange};
    #[test]
    fn counts_all_owned_text_and_removed_cell_rows() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        sheet
            .set_value("A1", CellValue::from(ExcelText::from_units(vec![0xd800])))
            .unwrap();
        sheet.get_mut("A1").unwrap().unwrap().style.number_format = Some("0.0".into());
        sheet.set_value("A2", "removed").unwrap();
        sheet.remove("A2").unwrap();
        workbook.named_ranges.push(NamedRange {
            name: "N".into(),
            formula: "S!A1".into(),
            sheet_scope: None,
        });
        let footprint = CloneFootprint::measure(&workbook).unwrap();
        assert_eq!(footprint.cells, 1);
        assert_eq!(footprint.rows, 2);
        assert_eq!(footprint.text_bytes, 1 + 2 + 3 + 1 + 4);
    }
    #[test]
    fn overlay_footprint_matches_staging_formula_clear_replacement_and_removed_rows() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        sheet
            .set_formula_with_cached_value("A1", "long formula", CellValue::Text("old text".into()))
            .unwrap();
        sheet.get_mut("A1").unwrap().unwrap().style.number_format = Some("0.000".into());
        sheet.set_value("A3", "removed").unwrap();
        sheet.remove("A3").unwrap();
        let assignments = vec![
            (0, "A1".parse().unwrap(), CellValue::Blank),
            (0, "A3".parse().unwrap(), CellValue::Text("new".into())),
            (0, "B4".parse().unwrap(), CellValue::Number(-0.)),
            (
                0,
                "A1".parse().unwrap(),
                CellValue::Text("replacement".into()),
            ),
        ];
        let expected = CloneFootprint::measure_with_assignments(&workbook, &assignments).unwrap();
        for (sheet, address, value) in assignments {
            workbook
                .sheet_mut(sheet)
                .unwrap()
                .set_value(address, value)
                .unwrap();
        }
        assert_eq!(expected, CloneFootprint::measure(&workbook).unwrap());
        assert_eq!(expected.rows, 3);
    }
}

#[cfg(test)]
mod spill_extent_accounting_tests {
    use super::*;
    #[test]
    fn saved_extent_source_identity_is_counted_and_removed_with_an_anchor_edit() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        let at = "A1".parse().unwrap();
        sheet.set_formula(at, "{7}").unwrap();
        sheet.set_spill_extent(at, at, "{7}").unwrap();
        assert_eq!(CloneFootprint::measure(&book).unwrap().text_bytes, 7);
        let assignments = [(0, at, 9.0.into())];
        let expected = CloneFootprint::measure_with_assignments(&book, &assignments).unwrap();
        book.sheet_mut(0).unwrap().set_value(at, 9.).unwrap();
        assert_eq!(expected, CloneFootprint::measure(&book).unwrap());
    }
}
