//! Checked retained payload/count accounting for callers before model cloning.
//! Counts describe model storage, not allocator overhead or resident memory.
use crate::Workbook;

/// Logical record counts and text bytes that a workbook clone would retain.
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
    /// Every owned textual payload cloned by Workbook: names, formulas, formats
    /// and cell text (including physical UTF-16 bytes).
    pub text_bytes: usize,
}
impl CloneFootprint {
    /// Exact storage accounting for resolved set_value assignments, without a clone.
    /// Later assignments to a coordinate replace earlier ones.
    pub fn measure_with_assignments(
        workbook: &Workbook,
        assignments: &[(usize, crate::CellAddress, crate::CellValue)],
    ) -> Option<Self> {
        let mut result = Self::measure(workbook)?;
        let mut final_values = std::collections::BTreeMap::new();
        for (sheet, address, value) in assignments {
            address.validate().ok()?;
            workbook.sheet(*sheet)?;
            final_values.insert((*sheet, *address), value);
        }
        let mut added_rows = std::collections::BTreeSet::new();
        for ((sheet, address), value) in final_values {
            let worksheet = workbook.sheet(sheet)?;
            if let Some(cell) = worksheet.get(address) {
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
            ..Self::default()
        };
        for sheet in &workbook.sheets {
            result.cells = result.cells.checked_add(sheet.cells().len())?;
            result.rows = result.rows.checked_add(sheet.rows.len())?;
            result.row_visibility = result.row_visibility.checked_add(sheet.visibility.len())?;
            result.text_bytes = result.text_bytes.checked_add(sheet.name.len())?;
            for cell in sheet.cells().values() {
                result.text_bytes = result
                    .text_bytes
                    .checked_add(cell.value.text_storage_bytes())?;
                result.text_bytes = result
                    .text_bytes
                    .checked_add(cell.formula.as_ref().map_or(0, String::len))?;
                result.text_bytes = result
                    .text_bytes
                    .checked_add(cell.style.number_format.as_ref().map_or(0, String::len))?;
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
    use crate::{CellValue, ExcelText, NamedRange};
    #[test]
    fn counts_all_owned_text_and_removed_cell_rows() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        sheet
            .set_value(
                "A1".parse().unwrap(),
                CellValue::from(ExcelText::from_units(vec![0xd800])),
            )
            .unwrap();
        sheet
            .get_mut("A1".parse().unwrap())
            .unwrap()
            .style
            .number_format = Some("0.0".into());
        sheet.set_value("A2".parse().unwrap(), "removed").unwrap();
        sheet.remove("A2".parse().unwrap());
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
            .set_formula_with_cached_value(
                "A1".parse().unwrap(),
                "long formula",
                CellValue::Text("old text".into()),
            )
            .unwrap();
        sheet
            .get_mut("A1".parse().unwrap())
            .unwrap()
            .style
            .number_format = Some("0.000".into());
        sheet.set_value("A3".parse().unwrap(), "removed").unwrap();
        sheet.remove("A3".parse().unwrap());
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
