//! Owned workbook mutations. Cache writes preserve formulas; literal writes replace them.
use crate::*;

/// One workbook mutation in a batch passed to [`Workbook::apply`].
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
    /// Update a formula's stored result only if its formula still matches.
    ///
    /// This checks formula text, not whether the supplied result is correct.
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
    /// Save an anchor result and its declared spill geometry under an exact source guard.
    SetSpillAnchorCachedValue {
        /// Worksheet containing the anchor.
        sheet: usize,
        /// Formula cell owning the saved footprint.
        anchor: CellAddress,
        /// Inclusive lower-right corner, including the anchor for one-cell arrays.
        last: CellAddress,
        /// Exact formula source required before publication.
        expected_formula: String,
        /// Saved anchor result.
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
    /// Update presentation metadata while preserving authored content and saved spill ownership.
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
    /// Rename a sheet without changing references in formulas.
    ///
    /// Use a [`RenameSheetPlan`] when references must follow the new name.
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
    /// Replace or remove cells as one batch without cloning the workbook.
    ///
    /// `None` removes a cell. Repeated coordinates use the last replacement.
    /// All addresses, sheets and formulas are validated before any mutation.
    /// Conflicting spill results are cleared as they are for individual cell edits.
    pub fn replace_cells(
        &mut self,
        cells: impl IntoIterator<Item = (usize, CellAddress, Option<Cell>)>,
    ) -> Result<(), ModelError> {
        let mut prepared = BTreeMap::<usize, BTreeMap<CellAddress, Option<Cell>>>::new();
        for (sheet, address, mut cell) in cells {
            address.validate()?;
            if sheet >= self.sheets.len() {
                return Err(ModelError("worksheet index out of bounds".into()));
            }
            if let Some(cell) = &mut cell
                && let Some(formula) = cell.formula.take()
            {
                cell.formula = Some(normalize_formula(formula)?);
            }
            if let Some(cell) = &cell {
                cell.validate_shared_formula()?;
            }
            prepared.entry(sheet).or_default().insert(address, cell);
        }
        // All fallible work precedes publication. Each changed sheet's saved
        // spill index is scanned once, rather than once per replacement.
        for (index, cells) in prepared {
            // SAFETY: Each index was checked against self.sheets.len() above. No sheets
            // are added or removed between validation and this lookup.
            let sheet = &mut self.sheets[index];
            sheet
                .spill_extents
                .retain(|anchor, _| !cells.contains_key(anchor));
            let invalidated = sheet
                .spill_cache
                .iter()
                .filter(|(at, _)| cells.contains_key(at))
                .map(|(_, spill)| spill.owner)
                .collect::<Vec<_>>();
            for anchor in invalidated {
                sheet.spill_extents.remove(&anchor);
            }
            let children = sheet
                .spill_cache
                .iter()
                .filter(|(address, cached)| {
                    cells.contains_key(address) || cells.contains_key(&cached.owner)
                })
                .map(|(address, _)| *address)
                .collect::<Vec<_>>();
            for child in children {
                if let Some(cached) = sheet.spill_cache.remove(&child) {
                    match cached.source {
                        Some(source) => {
                            sheet.cells.insert(child, source);
                        }
                        None => {
                            sheet.cells.remove(&child);
                        }
                    }
                }
            }
            for (address, cell) in cells {
                match cell {
                    Some(cell) => {
                        sheet.rows.insert(address.row);
                        sheet.cells.insert(address, cell);
                    }
                    None => {
                        sheet.cells.remove(&address);
                    }
                }
            }
        }
        Ok(())
    }

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
    /// Clone the source once and apply changes to that isolated copy.
    /// Failure leaves this source untouched; useful for exporting scenario state.
    pub fn with_changes(
        &self,
        changes: impl IntoIterator<Item = WorkbookChange>,
    ) -> Result<Self, ModelError> {
        let mut staged = self.clone();
        for change in changes {
            staged.apply_one(change)?;
        }
        Ok(staged)
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
            } => self.change_sheet(sheet)?.set_formula(address, formula)?,
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
            SetSpillAnchorCachedValue {
                sheet,
                anchor,
                last,
                expected_formula,
                value,
            } => {
                let worksheet = self.change_sheet(sheet)?;
                worksheet.set_spill_extent(anchor, last, &expected_formula)?;
                let cell = worksheet
                    .get_mut(anchor)
                    .ok_or_else(|| ModelError("missing spill anchor".into()))?;
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
                self.change_sheet(sheet)?.set_style(address, style)?;
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
    fn sparse_replacements_validate_the_entire_batch_before_publication() {
        let mut w = Workbook::new();
        w.add_sheet("S").unwrap().set_value(at("A1"), 1.).unwrap();
        let before = w.clone();
        let valid = (0, at("A1"), Some(Cell::new(2.)));
        for invalid in [
            (1, at("B1"), None),
            (
                0,
                CellAddress {
                    row: u32::MAX,
                    col: 0,
                },
                None,
            ),
            (
                0,
                at("B1"),
                Some(Cell {
                    formula: Some("= ".into()),
                    ..Default::default()
                }),
            ),
        ] {
            assert!(w.replace_cells([valid.clone(), invalid]).is_err());
            assert_eq!(w, before);
        }
        w.replace_cells([
            valid.clone(),
            (
                0,
                at("A1"),
                Some(Cell::with_formula("= 2+3", CellValue::Blank).unwrap()),
            ),
            (0, at("C1"), Some(Cell::new(9.))),
            (0, at("C1"), None),
        ])
        .unwrap();
        assert_eq!(
            w.sheet(0)
                .unwrap()
                .get(at("A1"))
                .unwrap()
                .formula
                .as_deref(),
            Some("2+3")
        );
        assert!(w.sheet(0).unwrap().get(at("C1")).is_none());
        let before = w.clone();
        w.replace_cells([]).unwrap();
        assert_eq!(w, before);
    }

    #[test]
    fn sparse_replacements_restore_spill_metadata_before_publishing_all_sources() {
        let mut w = Workbook::new();
        let s = w.add_sheet("S").unwrap();
        s.set_formula_with_cached_value(at("A1"), "{1,2,3}", 1.0.into())
            .unwrap();
        let style = CellStyle {
            number_format: Some("0.00".into()),
        };
        s.set(
            at("C1"),
            Cell {
                style: style.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        for (child, result) in [("B1", 2.), ("C1", 3.)] {
            s.set_spill_cached_value(at(child), at("A1"), "{1,2,3}", result.into())
                .unwrap();
        }
        w.replace_cells([
            (
                0,
                at("A1"),
                Some(Cell::with_formula("{4}", CellValue::Blank).unwrap()),
            ),
            (0, at("B1"), Some(Cell::new(99.))),
        ])
        .unwrap();
        let s = w.sheet(0).unwrap();
        assert_eq!(s.get(at("B1")).unwrap().value, CellValue::Number(99.));
        assert!(s.spill_owner(at("B1")).is_none());
        assert_eq!(s.get(at("C1")).unwrap().value, CellValue::Blank);
        assert_eq!(s.source_cell(at("C1")).unwrap().style, style);
        assert!(s.spill_owner(at("C1")).is_none());
    }

    #[test]
    fn failed_batches_leave_source_and_saved_values_untouched() {
        let mut w = Workbook::new();
        w.add_sheet("S")
            .unwrap()
            .set_formula_with_cached_value(at("A1"), "1+1", CellValue::Number(2.))
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
        s.set_formula(at("A1"), "{1,2}").unwrap();
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
    sheet
        .set_formula_with_cached_value(anchor, "{1,2}", 1.0.into())
        .unwrap();
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

#[cfg(test)]
mod spill_extent_tests {
    use super::*;
    fn at(s: &str) -> CellAddress {
        s.parse().unwrap()
    }
    #[test]
    fn saved_extent_tracks_singletons_completeness_source_guards_and_original_metadata() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet
            .set_formula_with_cached_value(at("A1"), "{7}", 7.0.into())
            .unwrap();
        sheet.set_spill_extent(at("A1"), at("A1"), "{7}").unwrap();
        assert_eq!(sheet.complete_spill_extent(at("A1")), Some(at("A1")));
        sheet.get_mut(at("A1")).unwrap().formula = Some("{8}".into());
        assert_eq!(sheet.spill_extent(at("A1")), None);
        assert_eq!(sheet.complete_spill_extent(at("A1")), None);
        assert!(sheet.set_spill_extent(at("A1"), at("B2"), "{7}").is_err());
        sheet.set_spill_extent(at("A1"), at("B2"), "{8}").unwrap();
        assert_eq!(sheet.complete_spill_extent(at("A1")), None);
        for child in ["B1", "A2", "B2"] {
            sheet
                .set_spill_cached_value(at(child), at("A1"), "{8}", 8.0.into())
                .unwrap();
        }
        assert_eq!(sheet.complete_spill_extent(at("A1")), Some(at("B2")));
        let bytes = postcard::to_allocvec(&snapshot::Snapshot::capture(&book)).unwrap();
        let restored: snapshot::Snapshot<'static> = postcard::from_bytes(&bytes).unwrap();
        let mut restored = restored.restore().unwrap();
        assert_eq!(
            restored.sheet(0).unwrap().complete_spill_extent(at("A1")),
            Some(at("B2"))
        );
        restored.replace_cells([(0, at("B2"), None)]).unwrap();
        assert_eq!(restored.sheet(0).unwrap().spill_extent(at("A1")), None);
        restored.sheet_mut(0).unwrap().clear_spill_cache(at("A1"));
        assert!(restored.sheet(0).unwrap().get(at("B1")).is_none());
    }
    #[test]
    fn sparse_huge_incomplete_geometry_does_not_expand_or_appear_complete() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_formula(at("A1"), "{1,2}").unwrap();
        sheet
            .set_spill_extent(at("A1"), at("XFD1048576"), "{1,2}")
            .unwrap();
        sheet
            .set_spill_cached_value(at("XFD1048576"), at("A1"), "{1,2}", 2.0.into())
            .unwrap();
        assert_eq!(sheet.spill_extent(at("A1")), Some(at("XFD1048576")));
        assert_eq!(sheet.complete_spill_extent(at("A1")), None);
        assert_eq!(sheet.cells().len(), 2);
        assert!(
            sheet
                .set_spill_extent(
                    at("A1"),
                    CellAddress {
                        row: MAX_ROWS,
                        col: 0
                    },
                    "{1,2}"
                )
                .is_err()
        );
        assert!(
            sheet
                .set_spill_extent(at("A1"), at("A1"), "changed")
                .is_err()
        );
    }
    #[test]
    fn guarded_anchor_change_is_transactional_and_child_edits_remove_geometry_authority() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula(at("A1"), "{1}")
            .unwrap();
        let before = book.clone();
        assert!(
            book.apply([WorkbookChange::SetSpillAnchorCachedValue {
                sheet: 0,
                anchor: at("A1"),
                last: at("B2"),
                expected_formula: "changed".into(),
                value: 1.0.into()
            }])
            .is_err()
        );
        assert_eq!(book, before);
        book.apply([WorkbookChange::SetSpillAnchorCachedValue {
            sheet: 0,
            anchor: at("A1"),
            last: at("A1"),
            expected_formula: "{1}".into(),
            value: 1.0.into(),
        }])
        .unwrap();
        assert_eq!(
            book.sheet(0).unwrap().complete_spill_extent(at("A1")),
            Some(at("A1"))
        );
        book.sheet_mut(0).unwrap().set_value(at("A1"), 7.).unwrap();
        assert_eq!(book.sheet(0).unwrap().spill_extent(at("A1")), None);
    }
}
