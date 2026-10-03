//! Owned workbook mutations. Cache writes preserve formulas; literal writes replace them.
use crate::*;

/// Owned source and saved-cache mutations consumed atomically by Workbook::apply.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum WorkbookChange {
    /// Replace a source cell with a literal, clearing its formula.
    SetValue {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Literal or saved result to store, preserving exact text/numeric payloads.
        #[serde(with = "snapshot::value_serde")]
        value: CellValue,
    },
    /// Replace formula source and clear its saved result.
    SetFormula {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Formula source to normalize and store.
        formula: String,
    },
    /// Saved file data, not validated evaluator provenance. Reject a different source.
    SetCachedValue {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Exact source identity required before applying a saved result.
        expected_formula: String,
        /// Literal or saved result to store, preserving exact text/numeric payloads.
        #[serde(with = "snapshot::value_serde")]
        value: CellValue,
    },
    /// Remove derived children owned by a spill anchor.
    ClearSpillCache {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Formula cell owning this derived spill cache.
        anchor: CellAddress,
    },
    /// Save one derived child while retaining its anchor and formula identity.
    SetSpillCachedValue {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Formula cell owning this derived spill cache.
        anchor: CellAddress,
        /// Exact source identity required before applying a saved result.
        expected_formula: String,
        /// Literal or saved result to store, preserving exact text/numeric payloads.
        #[serde(with = "snapshot::value_serde")]
        value: CellValue,
    },
    /// Replace the complete authored cell, including style.
    SetCell {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Complete authored cell to store.
        cell: Cell,
    },
    /// Update presentation metadata while preserving authored content.
    SetStyle {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
        /// Presentation metadata to apply.
        style: CellStyle,
    },
    /// Remove authored content and any associated spill cache.
    RemoveCell {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Validated coordinate of the affected cell.
        address: CellAddress,
    },
    /// Append a worksheet with a validated unique name.
    AddSheet {
        /// Worksheet or defined-name spelling validated by the selected mutation.
        name: String,
    },
    /// Metadata rename only; use formula-aware edits when references must be rewritten.
    RenameSheet {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Worksheet or defined-name spelling validated by the selected mutation.
        name: String,
    },
    /// Change the workbook's date-serial epoch.
    SetDateSystem {
        /// Date-serial epoch to select.
        date_system: DateSystem,
    },
    /// Add or replace a formula-backed defined name in its scope.
    DefineName {
        /// Worksheet or defined-name spelling validated by the selected mutation.
        name: String,
        /// Formula source to normalize and store.
        formula: String,
        /// Local sheet index, or None for workbook scope.
        sheet_scope: Option<usize>,
    },
    /// Remove a defined name from the selected scope.
    RemoveName {
        /// Worksheet or defined-name spelling validated by the selected mutation.
        name: String,
        /// Local sheet index, or None for workbook scope.
        sheet_scope: Option<usize>,
    },
    /// Record an explicitly present row without creating cells.
    CreateRow {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Zero-based row index to change.
        row: u32,
    },
    /// Remove the row and its authored cells.
    RemoveRow {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Zero-based row index to change.
        row: u32,
    },
    /// Set manual/filter visibility metadata for a row.
    SetRowVisibility {
        /// Zero-based index of the worksheet to change.
        sheet: usize,
        /// Zero-based row index to change.
        row: u32,
        /// Manual and filtered visibility flags to store.
        visibility: RowVisibility,
    },
}
impl Workbook {
    /// Apply an owned batch atomically. Validation failure leaves the source untouched.
    pub fn apply(
        &mut self,
        changes: impl IntoIterator<Item = WorkbookChange>,
    ) -> Result<(), ModelError> {
        let mut staged = self.clone();
        for change in changes {
            staged.apply_one(change)?;
        }
        *self = staged;
        Ok(())
    }
    fn apply_one(&mut self, change: WorkbookChange) -> Result<(), ModelError> {
        use WorkbookChange::*;
        match change {
            SetValue {
                sheet,
                address,
                value,
            } => self.change_sheet(sheet)?.set_value(address, value)?,
            SetFormula {
                sheet,
                address,
                formula,
            } => self
                .change_sheet(sheet)?
                .set_formula(address, formula, CellValue::Blank)?,
            SetCachedValue {
                sheet,
                address,
                expected_formula,
                value,
            } => {
                address.validate()?;
                let cell = self
                    .change_sheet(sheet)?
                    .get_mut(address)
                    .ok_or_else(|| ModelError("cached-value target is absent".into()))?;
                if cell.formula.as_deref() != Some(expected_formula.as_str()) {
                    return Err(ModelError("cached-value formula source changed".into()));
                }
                cell.value = value;
            }
            ClearSpillCache { sheet, anchor } => {
                anchor.validate()?;
                self.change_sheet(sheet)?.clear_spill_cache(anchor);
            }
            SetSpillCachedValue {
                sheet,
                address,
                anchor,
                expected_formula,
                value,
            } => self.change_sheet(sheet)?.set_spill_cached_value(
                address,
                anchor,
                &expected_formula,
                value,
            )?,
            SetCell {
                sheet,
                address,
                cell,
            } => {
                self.change_sheet(sheet)?.set(address, cell)?;
            }
            SetStyle {
                sheet,
                address,
                style,
            } => {
                address.validate()?;
                let sheet = self.change_sheet(sheet)?;
                let mut cell = sheet.source_cell(address).cloned().unwrap_or_default();
                cell.style = style;
                sheet.set(address, cell)?;
            }
            RemoveCell { sheet, address } => {
                address.validate()?;
                self.change_sheet(sheet)?.remove(address);
            }
            AddSheet { name } => {
                self.add_sheet(name)?;
            }
            RenameSheet { sheet, name } => self.rename_sheet(sheet, name)?,
            SetDateSystem { date_system } => self.date_system = date_system,
            DefineName {
                name,
                formula,
                sheet_scope,
            } => self.define_name(name, formula, sheet_scope)?,
            RemoveName { name, sheet_scope } => {
                let mut selected = None;
                for (i, candidate) in self.named_ranges.iter().enumerate() {
                    if candidate.sheet_scope == sheet_scope
                        && identifiers::checked_matches(&candidate.name, &name)
                            .map_err(|e| ModelError(e.to_string()))?
                    {
                        selected = Some(i);
                        break;
                    }
                }
                if let Some(i) = selected {
                    // SAFETY: i comes from enumerating this vector, which has not changed since the search.
                    self.named_ranges.remove(i);
                }
            }
            CreateRow { sheet, row } => {
                self.change_sheet(sheet)?.create_row(row)?;
            }
            RemoveRow { sheet, row } => {
                self.change_sheet(sheet)?.remove_row(row)?;
            }
            SetRowVisibility {
                sheet,
                row,
                visibility,
            } => self
                .change_sheet(sheet)?
                .set_row_visibility(row, visibility)?,
        }
        Ok(())
    }
    fn change_sheet(&mut self, index: usize) -> Result<&mut Worksheet, ModelError> {
        self.sheet_mut(index)
            .ok_or_else(|| ModelError("worksheet index out of bounds".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(s: &str) -> CellAddress {
        s.parse().unwrap()
    }
    #[test]
    fn failed_batches_leave_source_and_saved_values_untouched() {
        let mut w = Workbook::new();
        w.add_sheet("S")
            .unwrap()
            .set_formula(at("A1"), "1+1", CellValue::Number(2.))
            .unwrap();
        let before = w.clone();
        assert!(
            w.apply(vec![
                WorkbookChange::SetValue {
                    sheet: 0,
                    address: at("B1"),
                    value: CellValue::Number(5.)
                },
                WorkbookChange::SetCachedValue {
                    sheet: 0,
                    address: at("A1"),
                    expected_formula: "changed".into(),
                    value: CellValue::Number(9.)
                }
            ])
            .is_err()
        );
        assert_eq!(w, before);
    }
    #[test]
    fn serde_changes_preserve_numeric_bits_and_unpaired_text_units() {
        let values = [
            CellValue::Number(-0.),
            CellValue::Number(f64::from_bits(0x7ff8000000000042)),
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        ];
        for value in values {
            let changes = vec![WorkbookChange::SetValue {
                sheet: 0,
                address: at("A1"),
                value: value.clone(),
            }];
            let bytes = postcard::to_allocvec(&changes).unwrap();
            let changes: Vec<WorkbookChange> = postcard::from_bytes(&bytes).unwrap();
            let mut w = Workbook::new();
            w.add_sheet("S").unwrap();
            w.apply(changes).unwrap();
            match (&value, &w.sheet(0).unwrap().get(at("A1")).unwrap().value) {
                (CellValue::Number(a), CellValue::Number(b)) => {
                    assert_eq!(a.to_bits(), b.to_bits())
                }
                (a, b) => assert_eq!(a, b),
            }
        }
    }
    #[test]
    fn spill_cache_preserves_source_metadata_and_snapshot_restoration() {
        let mut w = Workbook::new();
        let s = w.add_sheet("S").unwrap();
        s.set_formula(at("A1"), "{1,2}", CellValue::Blank).unwrap();
        s.set(
            at("B1"),
            Cell {
                style: CellStyle {
                    number_format: Some("0.00".into()),
                },
                ..Default::default()
            },
        )
        .unwrap();
        s.set_spill_cached_value(at("B1"), at("A1"), "{1,2}", CellValue::Number(2.))
            .unwrap();
        let bytes = postcard::to_allocvec(&w.snapshot()).unwrap();
        let snapshot = postcard::from_bytes(&bytes).unwrap();
        let mut restored = Workbook::from_snapshot(snapshot).unwrap();
        assert_eq!(w, restored);
        restored
            .apply(vec![WorkbookChange::ClearSpillCache {
                sheet: 0,
                anchor: at("A1"),
            }])
            .unwrap();
        let cell = restored.sheet(0).unwrap().get(at("B1")).unwrap();
        assert_eq!(cell.value, CellValue::Blank);
        assert_eq!(cell.style.number_format.as_deref(), Some("0.00"));
    }
}

#[cfg(test)]
#[test]
fn rejected_source_replacement_preserves_saved_spill_ownership() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let anchor = "A1".parse().unwrap();
    let child = "B1".parse().unwrap();
    sheet.set_formula(anchor, "{1,2}", 1.0.into()).unwrap();
    sheet
        .set_spill_cached_value(child, anchor, "{1,2}", 2.0.into())
        .unwrap();
    let invalid = Cell {
        formula: Some("= ".into()),
        ..Default::default()
    };
    assert!(sheet.set(anchor, invalid.clone()).is_err());
    assert!(sheet.set(child, invalid).is_err());
    assert_eq!(sheet.spill_owner(child), Some(anchor));
    assert_eq!(sheet.get(child).unwrap().value, CellValue::Number(2.));
    assert!(sheet.source_cell(child).is_none());
}
