//! Ordered worksheets, defined names, and workbook source metadata.
use crate::calculation::CalculationError;
use crate::files::{LoadBuilder, XlsxSaveBuilder};
use crate::model;
use crate::model::cell::normalize_formula;
use crate::model::worksheet::validate_sheet_name;
use crate::model::{
    Cell, CellAddress, EditOutcome, FileMetadata, FormulaSource, ModelError, RenameSheetPlan,
    SheetPresentation, SheetVisibility, StyleContext, WorkbookChange, Worksheet, cell_pages,
    identifiers, snapshot,
};
use poirot_io as io;
use poirot_io::NamedRange;
use std::collections::BTreeMap;
use std::path::Path;

use poirot_functions::scalar::DateSystem;
/// Editable workbook data: ordered sheets, named formulas, and a date system.
///
/// Stores authored formulas and saved results. Reading does not calculate; call
/// [`Self::recalculate`] to publish results, or use [`Self::overlay`] for scenarios.
/// [`Self::open`] and [`Self::save_xlsx`] handle workbook files.
///
/// ```rust
/// use poirot::model::Workbook;
///
/// let mut book = Workbook::new();
/// book.add_sheet("Inputs")?;
/// book.add_sheet("Summary")?;
/// book.define_name("Rate", "0.2", None)?;
/// book.define_name("Rate", "0.3", Some(0))?;
/// assert_eq!(book.defined_name("rate", 0).ok_or("missing local name")?.formula, "0.3");
/// assert_eq!(book.defined_name("rate", 1).ok_or("missing global name")?.formula, "0.2");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Workbook {
    /// Theme and indexed palette source context.
    pub style_context: StyleContext,
    /// Optional provenance and diagnostics retained from file import.
    pub file_metadata: Option<FileMetadata>,
    pub(super) sheets: Vec<Worksheet>,
    /// Epoch used to interpret numeric date/time serials.
    pub date_system: DateSystem,
    /// Defined-name sources in workbook order.
    pub named_ranges: Vec<NamedRange>,
}

impl Workbook {
    /// Create an empty workbook using the default date system.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow worksheets in workbook index order.
    pub fn sheets(&self) -> &[Worksheet] {
        &self.sheets
    }

    /// Borrow sheets mutably; callers must rebuild dependent compiled/runtime state.
    pub fn sheets_mut(&mut self) -> &mut [Worksheet] {
        &mut self.sheets
    }

    /// Borrow a worksheet by zero-based index.
    pub fn sheet(&self, index: usize) -> Option<&Worksheet> {
        self.sheets.get(index)
    }

    /// Borrow a worksheet mutably by zero-based index.
    pub fn sheet_mut(&mut self, index: usize) -> Option<&mut Worksheet> {
        self.sheets.get_mut(index)
    }

    /// Policy-aware lookup. A resolved match wins; otherwise retain unresolved mappings.
    pub fn try_sheet_index_by_name(
        &self,
        name: &str,
    ) -> Result<Option<usize>, identifiers::IdentifierError> {
        let mut diagnostic = None;
        for (index, sheet) in self.sheets.iter().enumerate() {
            match identifiers::checked_matches(sheet.name(), name) {
                Ok(true) => return Ok(Some(index)),
                Ok(false) => {}
                Err(error) => diagnostic = Some(error),
            }
        }
        diagnostic.map_or(Ok(None), Err)
    }

    /// Legacy conservative lookup; use `try_sheet_index_by_name` for diagnostics.
    pub fn sheet_index_by_name(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|sheet| identifiers::matches(sheet.name(), name))
    }

    /// Import a raw spelling without authoring validation or unresolved case rejection.
    /// Definite duplicate identities are still rejected; adapters report unresolved mappings.
    pub fn import_sheet(
        &mut self,
        name: impl Into<String>,
    ) -> Result<&mut Worksheet, io::ModelError> {
        let name = name.into();
        if self.sheet_index_by_name(&name).is_some() {
            return Err(io::ModelError::DuplicateWorksheetName { name });
        }
        self.sheets.push(Worksheet {
            name,
            presentation: SheetPresentation::default(),
            cells: cell_pages::CellStore::default(),
            rows: std::collections::BTreeSet::new(),
            visibility: BTreeMap::new(),
            spill_cache: BTreeMap::new(),
            spill_extents: BTreeMap::new(),
        });
        // SAFETY: This function just pushed a worksheet and has not removed any element before last_mut().
        Ok(self.sheets.last_mut().expect("inserted sheet"))
    }

    /// Find a sheet with the model's supported identifier comparison.
    pub fn sheet_by_name(&self, name: &str) -> Option<&Worksheet> {
        self.sheets
            .iter()
            .find(|s| identifiers::matches(&s.name, name))
    }

    /// Find a mutable sheet using the model's identifier comparison.
    pub fn sheet_by_name_mut(&mut self, name: &str) -> Option<&mut Worksheet> {
        self.sheets
            .iter_mut()
            .find(|s| identifiers::matches(&s.name, name))
    }

    /// Validate a unique sheet name and append an empty sparse worksheet.
    pub fn add_sheet(&mut self, name: impl Into<String>) -> Result<&mut Worksheet, ModelError> {
        let sheet = Worksheet::new(name)?;
        if self.try_sheet_index_by_name(sheet.name())?.is_some() {
            return Err(ModelError::DuplicateWorksheetName {
                name: sheet.name().into(),
            });
        }
        self.sheets.push(sheet);
        self.sheets
            .last_mut()
            .ok_or(ModelError::WorksheetInsertionMissing)
    }

    /// Rename source metadata after a caller has planned any required formula rewrites.
    pub(crate) fn rename_sheet_metadata(
        &mut self,
        index: usize,
        name: impl Into<String>,
    ) -> Result<(), ModelError> {
        let name = name.into();
        validate_sheet_name(&name)?;
        for (i, sheet) in self.sheets.iter().enumerate() {
            if i != index && identifiers::checked_matches(sheet.name(), &name)? {
                return Err(ModelError::DuplicateWorksheetName { name });
            }
        }
        let sheet = self
            .sheet_mut(index)
            .ok_or(ModelError::InvalidWorksheetIndex { index })?;
        sheet.name = name;
        Ok(())
    }

    /// Remove worksheet metadata and its local defined names; adjust later scopes.
    /// Callers must rewrite surviving formulas before publishing this mutation.
    pub fn remove_sheet(&mut self, index: usize) -> Result<Worksheet, ModelError> {
        if index >= self.sheets.len() {
            return Err(ModelError::InvalidWorksheetIndex { index });
        }
        // SAFETY: The explicit check above proves index is in the current vector.
        let removed = self.sheets.remove(index);
        self.named_ranges.retain(|n| n.sheet_scope != Some(index));
        for name in &mut self.named_ranges {
            if let Some(scope) = &mut name.sheet_scope
                && *scope > index
            {
                *scope -= 1;
            }
        }
        Ok(removed)
    }

    /// Move a sheet to a final zero-based position, remapping local name scopes.
    /// Callers must rebuild compiled references so 3D spans follow the new order.
    pub fn move_sheet(&mut self, from: usize, to: usize) -> Result<(), ModelError> {
        if from >= self.sheets.len() || to >= self.sheets.len() {
            return Err(ModelError::InvalidWorksheetIndex {
                index: if from >= self.sheets.len() { from } else { to },
            });
        }
        if from == to {
            return Ok(());
        }
        // SAFETY: from was checked against this vector's current length.
        let sheet = self.sheets.remove(from);
        // SAFETY: to was less than the original length; removal decreased it by one.
        self.sheets.insert(to, sheet);
        for name in &mut self.named_ranges {
            if let Some(scope) = &mut name.sheet_scope {
                *scope = if *scope == from {
                    to
                } else if from < to && *scope > from && *scope <= to {
                    scope.saturating_sub(1)
                } else if to < from && *scope >= to && *scope < from {
                    scope.saturating_add(1)
                } else {
                    *scope
                };
            }
        }
        Ok(())
    }

    /// Append a typed sheet copy and copies of its local names. Unqualified formulas
    /// bind the copy; explicitly qualified references keep their named-sheet identity.
    /// Evaluator caches must be rebuilt by the caller.
    pub fn duplicate_sheet(
        &mut self,
        index: usize,
        name: impl Into<String>,
    ) -> Result<usize, ModelError> {
        let name = name.into();
        validate_sheet_name(&name)?;
        if self.try_sheet_index_by_name(&name)?.is_some() {
            return Err(ModelError::DuplicateWorksheetName { name });
        }
        let mut copy = self
            .sheet(index)
            .ok_or(ModelError::InvalidWorksheetIndex { index })?
            .clone();
        copy.name = name;
        copy.presentation.visibility = SheetVisibility::Visible;
        let target = self.sheets.len();
        let names: Vec<_> = self
            .named_ranges
            .iter()
            .filter(|n| n.sheet_scope == Some(index))
            .cloned()
            .map(|mut n| {
                n.sheet_scope = Some(target);
                n
            })
            .collect();
        self.sheets.push(copy);
        self.named_ranges.extend(names);
        Ok(target)
    }

    /// Normalize and store a defined-name formula in the selected scope.
    pub fn define_name(
        &mut self,
        name: impl Into<String>,
        formula: impl Into<String>,
        sheet_scope: Option<usize>,
    ) -> Result<(), ModelError> {
        let name = name.into();
        if name.is_empty()
            || name.encode_utf16().count() > 255
            || !name.chars().enumerate().all(|(i, c)| {
                c.is_alphabetic()
                    || c == '_'
                    || c == '\\'
                    || (i > 0 && (c.is_ascii_digit() || c == '.'))
            })
            || name.parse::<CellAddress>().is_ok()
            || name.eq_ignore_ascii_case("R")
            || name.eq_ignore_ascii_case("C")
        {
            return Err(ModelError::InvalidDefinedName { name });
        }
        if let Some(index) = sheet_scope
            && index >= self.sheets.len()
        {
            return Err(ModelError::InvalidWorksheetIndex { index });
        }
        for existing in self
            .named_ranges
            .iter()
            .filter(|n| n.sheet_scope == sheet_scope)
        {
            if identifiers::checked_matches(&existing.name, &name)? {
                return Err(ModelError::DuplicateDefinedName { name, sheet_scope });
            }
        }
        self.named_ranges.push(NamedRange {
            name,
            formula: normalize_formula(formula.into())?,
            sheet_scope,
        });
        Ok(())
    }

    /// Resolve local scope before global scope; unresolved local mappings block fallback.
    pub fn try_defined_name(
        &self,
        name: &str,
        sheet: usize,
    ) -> Result<Option<&NamedRange>, identifiers::IdentifierError> {
        for scope in [Some(sheet), None] {
            let mut diagnostic = None;
            for existing in self.named_ranges.iter().filter(|n| n.sheet_scope == scope) {
                match identifiers::checked_matches(&existing.name, name) {
                    Ok(true) => return Ok(Some(existing)),
                    Ok(false) => {}
                    Err(error) => diagnostic = Some(error),
                }
            }
            if let Some(error) = diagnostic {
                return Err(error);
            }
        }
        Ok(None)
    }

    /// Legacy conservative lookup; use `try_defined_name` for diagnostics.
    pub fn defined_name(&self, name: &str, sheet: usize) -> Option<&NamedRange> {
        self.named_ranges
            .iter()
            .find(|n| n.sheet_scope == Some(sheet) && identifiers::matches(&n.name, name))
            .or_else(|| {
                self.named_ranges
                    .iter()
                    .find(|n| n.sheet_scope.is_none() && identifiers::matches(&n.name, name))
            })
    }
}

impl Workbook {
    /// Borrow serializable workbook data without requiring runtime storage to implement Serde.
    pub fn snapshot(&self) -> snapshot::Snapshot<'_> {
        snapshot::Snapshot::capture(self)
    }

    /// Restore owned model state from its Serde snapshot representation.
    pub fn from_snapshot(snapshot: snapshot::Snapshot<'_>) -> Result<Self, ModelError> {
        snapshot.restore()
    }
}

impl Workbook {
    /// Open an XLS or XLSX file, detecting its format from the contents.
    ///
    /// Import warnings are kept in the workbook's file metadata.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, io::IoError> {
        Self::load().open(path)
    }

    /// Read an XLS or XLSX workbook into owned data, independent of the input buffer.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, io::IoError> {
        Self::load().from_bytes(bytes)
    }

    /// Configure XLS or XLSX loading.
    pub fn load() -> LoadBuilder {
        LoadBuilder::default()
    }

    /// Configure selected-output calculation with one captured default context.
    /// The scenario borrows this workbook; use [`Self::into_evaluation`] to move it instead.
    pub fn evaluation(&self) -> crate::calculation::EvaluationBuilder<'_> {
        crate::calculation::EvaluationBuilder::new(self)
    }

    /// Configure incremental scenarios that preserve unaffected results across edits.
    /// The scenario borrows this workbook; use [`Self::into_overlay`] to move it instead.
    pub fn overlay(&self) -> crate::calculation::OverlayBuilder<'_> {
        crate::calculation::OverlayBuilder::new(self)
    }

    /// Move the source into a direct scenario without copying workbook data.
    #[inline]
    pub fn into_evaluation(
        self,
    ) -> crate::calculation::EvaluationBuilder<'static, model::Workbook> {
        crate::calculation::EvaluationBuilder::new(self)
    }

    /// Move the source into an incremental scenario without copying workbook data.
    #[inline]
    pub fn into_overlay(self) -> crate::calculation::OverlayBuilder<'static, model::Workbook> {
        crate::calculation::OverlayBuilder::new(self)
    }

    /// Configure usable snapshot restoration with compatible reuse or default source recovery.
    pub fn restore_snapshot() -> crate::snapshots::RestoreBuilder {
        crate::snapshots::RestoreBuilder::default()
    }

    /// Inspect authored content and saved values without calculating.
    ///
    /// Accepts worksheet-local A1 text, a typed address, or zero-based `(row, column)` integers.
    /// Invalid coordinates return an error, including when the sheet is missing.
    /// Missing sheets and unstored cells return `Ok(None)`; explicitly stored blanks
    /// return `Ok(Some(_))`. Names use the same comparison as [`Self::sheet_by_name`].
    /// Resolve the sheet first when missing sheets must differ from unstored cells.
    pub fn stored_cell<A>(
        &self,
        sheet: &str,
        address: A,
    ) -> Result<Option<&model::Cell>, ModelError>
    where
        A: TryInto<model::CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        Ok(self
            .sheet_by_name(sheet)
            .and_then(|sheet| sheet.get_validated(address)))
    }

    /// Calculate the workbook and update its stored cell values.
    ///
    /// Use [`crate::calculation::Evaluation`] to query selected outputs with scenario inputs,
    /// or [`crate::calculation::Overlay`] to reuse unaffected calculations across edits.
    ///
    /// If calculation fails, none of the calculated values are written. Edits made
    /// before this call remain. Spreadsheet errors such as `#DIV/0!` are stored as
    /// cell values and do not make this method return an error.
    pub fn recalculate(&mut self) -> Result<(), CalculationError> {
        self.recalculate_with_context(crate::calculation::EvaluationContext::default())
    }

    /// Calculate the workbook with a supplied clock, random seed, locale, and limits.
    ///
    /// This has the same result and failure behavior as [`Self::recalculate`].
    pub fn recalculate_with_context(
        &mut self,
        context: crate::calculation::EvaluationContext,
    ) -> Result<(), CalculationError> {
        self.recalculate_with_policy(context, crate::calculation::EvaluationPolicy::default())
    }

    /// Recalculate using explicit producer, array, and compatibility policies.
    /// Caller-trusted producers consume saved results and reject incomplete saved arrays.
    /// All cache writes are atomic; input edits remain if evaluation fails.
    pub fn recalculate_with_policy(
        &mut self,
        context: crate::calculation::EvaluationContext,
        policy: crate::calculation::EvaluationPolicy,
    ) -> Result<(), CalculationError> {
        let changes = {
            let mut evaluation = crate::eval::overlay::Evaluation::with_workbook(&*self)
                .with_policy(policy)
                .build_calculation(context)
                .map_err(CalculationError::Preparation)?;
            evaluation.all().map_err(CalculationError::Evaluation)?;
            evaluation.into_changes()
        };
        self.apply(changes).map_err(CalculationError::Model)
    }

    /// Configure a rename of `sheet` (zero-based) that also updates references to it
    /// in cell formulas and defined names. Call `apply` to perform the edit.
    pub fn rename_sheet(
        &mut self,
        sheet: usize,
        name: impl Into<String>,
    ) -> crate::editing::RenameSheetBuilder<'_> {
        crate::editing::RenameSheetBuilder::new(self, sheet, name.into())
    }

    /// Configure in-place translation of references within existing formulas.
    /// Call `apply` to perform the edit.
    pub fn translate_formulas(&mut self) -> crate::editing::TranslateFormulasBuilder<'_> {
        crate::editing::TranslateFormulasBuilder::new(self)
    }

    /// Configure a native Poirot snapshot of this workbook.
    pub fn save_snapshot(&self) -> crate::eval::artifact::SnapshotSaveBuilder<'_> {
        crate::eval::artifact::SnapshotSaveBuilder::workbook(self)
    }

    /// Configure an XLSX save. Saving does not recalculate formulas.
    pub fn save_xlsx(&self) -> XlsxSaveBuilder<'_> {
        XlsxSaveBuilder::borrowed(self)
    }
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
    ) -> Result<(), io::ModelError> {
        let mut prepared = BTreeMap::<usize, BTreeMap<CellAddress, Option<Cell>>>::new();
        for (sheet, address, mut cell) in cells {
            address.validate()?;
            if sheet >= self.sheets.len() {
                return Err(io::ModelError::InvalidWorksheetIndex { index: sheet });
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
        use WorkbookChange::{
            AddSheet, ClearSpillCache, CreateRow, DefineName, RemoveCell, RemoveName, RemoveRow,
            RenameSheetMetadata, SetCachedValue, SetCell, SetDateSystem, SetFormula,
            SetRowVisibility, SetSpillAnchorCachedValue, SetSpillCachedValue, SetStyle, SetValue,
        };
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
                    .get_mut(address)?
                    .ok_or(ModelError::MissingCachedValueTarget { sheet, address })?;
                if cell.formula.as_deref() != Some(expected_formula.as_str()) {
                    return Err(ModelError::CachedValueFormulaChanged { sheet, address });
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
                    .get_mut(anchor)?
                    .ok_or(ModelError::MissingSpillAnchor { sheet, anchor })?;
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
                self.change_sheet(sheet)?.remove(address)?;
            }
            AddSheet { name } => {
                self.add_sheet(name)?;
            }
            RenameSheetMetadata { sheet, name } => self.rename_sheet_metadata(sheet, name)?,
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
                        && identifiers::checked_matches(&candidate.name, &name)?
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
            .ok_or(ModelError::InvalidWorksheetIndex { index })
    }
}

impl Workbook {
    /// Apply a sheet rename and its formula replacements.
    ///
    /// Rejects a stale plan before changing the workbook. Stored results and styles
    /// are preserved.
    pub(crate) fn apply_rename_sheet_plan(
        &mut self,
        plan: &RenameSheetPlan,
    ) -> Result<EditOutcome, ModelError> {
        if !crate::model::edit::same_source(self, &plan.expected) {
            return Err(ModelError::StaleRenamePlan);
        }
        let previous_name = self
            .sheets
            .get(plan.sheet)
            .ok_or(ModelError::InvalidWorksheetIndex { index: plan.sheet })?
            .name
            .clone();
        // The source comparison above ensures that the indices and formula text
        // checked when the plan was built still refer to the same content.
        self.sheets
            .get_mut(plan.sheet)
            .ok_or(ModelError::InvalidWorksheetIndex { index: plan.sheet })?
            .name
            .clone_from(&plan.name);
        for edit in &plan.replacements {
            match edit.source {
                FormulaSource::Cell { sheet, address } => {
                    let cell = self
                        .sheets
                        .get_mut(sheet)
                        .and_then(|worksheet| worksheet.cells.get_mut(&address))
                        .ok_or(ModelError::MissingRenameCell { sheet, address })?;
                    cell.formula = Some(edit.replacement.clone());
                    cell.shared_formula = None;
                }
                FormulaSource::DefinedName { index } => self
                    .named_ranges
                    .get_mut(index)
                    .ok_or(ModelError::MissingRenameDefinedName { index })?
                    .formula
                    .clone_from(&edit.replacement),
            }
        }
        Ok(EditOutcome {
            renamed_sheet: plan.sheet,
            previous_name,
            new_name: plan.name.clone(),
            changed_sources: plan.replacements.iter().map(|e| e.source.clone()).collect(),
        })
    }
}

#[cfg(test)]
mod sheet_access_tests {
    use super::*;

    #[test]
    fn explicit_sheet_access_and_stored_cell_preserve_absence() {
        let mut workbook = Workbook::new();
        assert!(workbook.sheet(0).is_none());
        assert!(workbook.sheet_mut(0).is_none());
        assert!(workbook.sheet_by_name("Data").is_none());
        assert!(workbook.sheet_by_name_mut("Data").is_none());
        assert_eq!(workbook.sheet_index_by_name("Data"), None);
        workbook.add_sheet("Data").unwrap();
        assert!(workbook.add_sheet("Data").is_err());
        assert_eq!(workbook.sheets().len(), 1);
        assert_eq!(workbook.sheets_mut().len(), 1);
        assert_eq!(workbook.sheet_index_by_name("data"), Some(0));
        let address = "A1".parse::<crate::model::CellAddress>().unwrap();
        assert!(workbook.stored_cell("Data", address).unwrap().is_none());
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address, model::CellValue::Blank)
            .unwrap();
        assert!(workbook.stored_cell("Data", address).unwrap().is_some());
        assert!(workbook.stored_cell("Missing", address).unwrap().is_none());
    }
}
