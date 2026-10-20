//! Store and edit workbook data.
//!
//! [`Workbook`] holds sparse worksheets, cell values, formulas, named formulas, and
//! a date system. [`Worksheet`] provides cell access through zero-based [`CellAddress`]
//! coordinates.
//!
//! A formula cell stores both its formula and a saved result. Changing workbook data
//! does not calculate a new result. This crate has no file reader or formula evaluator;
//! use [poirot](https://docs.rs/poirot) for those workflows.
use std::{collections::BTreeMap, fmt, str::FromStr};

mod numeric_fidelity;
// Numeric file-representation checks shared by import and export adapters.
pub use numeric_fidelity::*;
mod numeric_bits;

// Exact binary64 identity and pinned source exceptions retained across evaluation boundaries.
pub use numeric_bits::{CommonsConvergence, IeeeNumberBits, IeeeNumberClass, KnownSourceException};
// Shared scalar, exact-text, and error values used by source cells and formula kernels.
pub use poirot_functions_core::{
    CellError, CellValue, ExcelText, TextRef, TextUnits, TextUnitsError, ValueError as ModelError,
};
mod cell_pages;
// Generic sparse storage and ordered views used by workbook and overlay grids.
pub use cell_pages::{CellRange, CellStore, ColumnCells, OccupiedRows, WorksheetCells};
mod rows;
// Borrowed row access and explicit visibility metadata.
pub use rows::{Row, RowMut, RowVisibility};
mod changes;
/// Inspection of the source data copied when cloning a workbook.
pub mod clone_footprint;
mod edit;
/// Owned source/cache mutations applied atomically to a workbook.
pub use changes::WorkbookChange;
/// Checked case-insensitive comparison for worksheet and defined-name identities.
pub mod identifiers;
/// Bounded sparse query views that distinguish stored blanks from absent coordinates.
pub mod query;
// Source-checked formula replacements used by structural workbook edits.
pub use edit::{EditOutcome, FormulaReplacement, FormulaSource, RenameSheetPlan};

/// Exclusive worksheet row bound shared by addresses and adapters.
pub const MAX_ROWS: u32 = 1_048_576;
/// Exclusive worksheet column bound shared by addresses and adapters.
pub const MAX_COLUMNS: u32 = 16_384;

/// A cell position with zero-based row and column indices.
///
/// Parse A1 notation with `"B3".parse()`, or use [`Self::new`] with `(2, 1)`.
/// Public fields also allow unchecked construction; [`Self::validate`] checks bounds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct CellAddress {
    /// Zero-based row coordinate.
    pub row: u32,
    /// Zero-based column coordinate.
    pub col: u32,
}
impl CellAddress {
    /// Construct an address only if both axes fit worksheet bounds.
    pub fn new(row: u32, col: u32) -> Result<Self, ModelError> {
        let a = Self { row, col };
        a.validate()?;
        Ok(a)
    }
    /// Check that the row and column fit within Excel worksheet limits.
    pub fn validate(self) -> Result<(), ModelError> {
        if self.row >= MAX_ROWS || self.col >= MAX_COLUMNS {
            Err(ModelError(
                "cell address exceeds Excel worksheet limits".into(),
            ))
        } else {
            Ok(())
        }
    }
}
impl fmt::Display for CellAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut col = u64::from(self.col) + 1;
        let mut letters = Vec::new();
        while col > 0 {
            letters.push((b'A' + ((col - 1) % 26) as u8) as char);
            col = (col - 1) / 26;
        }
        for c in letters.iter().rev() {
            write!(f, "{c}")?;
        }
        write!(f, "{}", u64::from(self.row) + 1)
    }
}
impl FromStr for CellAddress {
    /// Model diagnostic for invalid A1 syntax or worksheet bounds.
    type Err = ModelError;
    fn from_str(s: &str) -> Result<Self, ModelError> {
        let invalid = || ModelError(format!("invalid A1 cell address: {s}"));
        let s = s.strip_prefix('$').unwrap_or(s);
        let split = s
            .find(|c: char| !c.is_ascii_alphabetic())
            .ok_or_else(invalid)?;
        if split == 0 {
            return Err(invalid());
        }
        let mut col = 0u32;
        // SAFETY: split comes from str::find on a character predicate, so it is an in-bounds UTF-8 boundary.
        for b in s[..split].bytes() {
            col = col
                .checked_mul(26)
                .and_then(|v| v.checked_add(u32::from(b.to_ascii_uppercase() - b'A') + 1))
                .ok_or_else(invalid)?;
        }
        // SAFETY: split comes from str::find on a character predicate, so it is an in-bounds UTF-8 boundary.
        let row_text = s[split..].strip_prefix('$').unwrap_or(&s[split..]);
        if row_text.is_empty() || !row_text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let row: u32 = row_text.parse().map_err(|_| invalid())?;
        Self::new(row.checked_sub(1).ok_or_else(invalid)?, col - 1)
    }
}
/// Cell presentation metadata.
///
/// A number format changes display, not the stored value. Absence selects the
/// consumer's default format.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CellStyle {
    /// Optional Excel format spelling; affects display rather than stored value.
    pub number_format: Option<String>,
}
/// A stored value, optional formula, and display metadata.
///
/// For formula cells, `value` is the saved result and can be stale until recalculation.
/// For literal cells, it is the input value.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    /// Literal input or potentially stale saved formula result.
    #[serde(with = "snapshot::value_serde")]
    pub value: CellValue,
    /// Normalized formula source; None denotes a literal cell.
    pub formula: Option<String>,
    /// Presentation metadata retained independently of calculation.
    pub style: CellStyle,
}
impl Cell {
    /// Create an unstyled literal cell.
    pub fn new(value: impl Into<CellValue>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }
    /// Create a formula cell with a caller-supplied stored result.
    ///
    /// Trims whitespace and an optional leading `=`. An empty formula is rejected;
    /// the stored result is not calculated or checked against the formula.
    pub fn with_formula(formula: impl Into<String>, cached: CellValue) -> Result<Self, ModelError> {
        Ok(Self {
            value: cached,
            formula: Some(normalize_formula(formula.into())?),
            style: CellStyle::default(),
        })
    }
}
fn normalize_formula(formula: String) -> Result<String, ModelError> {
    let text = formula
        .trim()
        .strip_prefix('=')
        .unwrap_or(formula.trim())
        .trim();
    if text.is_empty() {
        Err(ModelError("formula must not be empty".into()))
    } else {
        Ok(text.into())
    }
}
fn validate_sheet_name(name: &str) -> Result<(), ModelError> {
    if name.is_empty()
        || name.encode_utf16().count() > 31
        || name
            .chars()
            .any(|c| matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']' | '\0' | '\u{3}'))
        || name.starts_with('\'')
        || name.ends_with('\'')
    {
        Err(ModelError(format!("invalid worksheet name: {name}")))
    } else {
        Ok(())
    }
}
/// Persisted worksheet visibility; imports may retain very-hidden sheets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SheetVisibility {
    /// Ordinary visible worksheet.
    #[default]
    Visible,
    /// Hidden worksheet, available to an unhide control.
    Hidden,
    /// Workbook author restricted visibility, preserved on import/export.
    VeryHidden,
}
/// Workbook-owned presentation independent of row visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct SheetPresentation {
    /// Saved sheet visibility.
    pub visibility: SheetVisibility,
    /// Explicit sRGB channels; no palette/theme approximation is performed.
    pub tab_color: Option<[u8; 3]>,
}
/// A named worksheet with stored cells and explicit row records.
///
/// Missing coordinates allocate no cell; [`Worksheet::get`] returns `None` for them.
/// Setting a value clears its formula. Setting a formula starts with a blank saved
/// result; use [`Worksheet::set_formula_with_cached_value`] to supply one explicitly.
#[derive(Debug, Clone, PartialEq)]
pub struct Worksheet {
    name: String,
    presentation: SheetPresentation,
    cells: cell_pages::CellStore,
    rows: std::collections::BTreeSet<u32>,
    visibility: BTreeMap<u32, RowVisibility>,
    spill_cache: BTreeMap<CellAddress, SpillCell>,
}
impl Worksheet {
    /// Create an empty sparse sheet after validating its name.
    pub fn new(name: impl Into<String>) -> Result<Self, ModelError> {
        let name = name.into();
        validate_sheet_name(&name)?;
        Ok(Self {
            name,
            presentation: SheetPresentation::default(),
            cells: cell_pages::CellStore::default(),
            rows: std::collections::BTreeSet::new(),
            visibility: BTreeMap::new(),
            spill_cache: BTreeMap::new(),
        })
    }
    /// Borrow the validated worksheet name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Read durable sheet metadata.
    pub fn presentation(&self) -> SheetPresentation {
        self.presentation
    }
    /// Replace durable metadata. Importers may preserve all-hidden workbooks.
    pub fn set_presentation(&mut self, presentation: SheetPresentation) {
        self.presentation = presentation;
    }
    /// Borrow the coordinate-ordered view of stored cells.
    pub fn cells(&self) -> WorksheetCells<'_> {
        self.cells.cells()
    }
    /// Read a stored cell, or return `None` if the position has no cell.
    ///
    /// An explicitly stored blank returns `Some`, just like any other cell.
    pub fn get(&self, address: CellAddress) -> Option<&Cell> {
        self.cells.get(&address)
    }
    /// Mutably borrow a stored cell.
    ///
    /// Direct edits bypass formula normalization and spill cleanup. Use
    /// [`Self::set_value`] or [`Self::set_formula`] when replacing cell contents.
    pub fn get_mut(&mut self, address: CellAddress) -> Option<&mut Cell> {
        self.cells.get_mut(&address)
    }
    /// Replace a cell, returning the previous cell if one was stored.
    ///
    /// Validates the address and formula, then clears any conflicting spill results.
    pub fn set(&mut self, address: CellAddress, cell: Cell) -> Result<Option<Cell>, ModelError> {
        address.validate()?;
        let mut cell = cell;
        if let Some(f) = cell.formula.take() {
            cell.formula = Some(normalize_formula(f)?);
        }
        self.clear_spill_cache(address);
        self.spill_cache.remove(&address);
        self.rows.insert(address.row);
        Ok(self.cells.insert(address, cell))
    }
    /// Store a literal value and clear any formula at this address, retaining its style.
    pub fn set_value(
        &mut self,
        address: CellAddress,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        address.validate()?;
        self.clear_spill_cache(address);
        self.spill_cache.remove(&address);
        self.rows.insert(address.row);
        if self.get(address).is_none() {
            self.cells.insert(address, Cell::default());
        }
        let cell = self
            .cells
            .get_mut(&address)
            .ok_or_else(|| ModelError("cell insertion failed".into()))?;
        cell.value = value.into();
        cell.formula = None;
        Ok(())
    }
    /// Store a formula and clear its saved result, preserving the cell's style.
    /// A leading `=` is optional. This does not evaluate the formula.
    pub fn set_formula(
        &mut self,
        address: CellAddress,
        formula: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.set_formula_with_cached_value(address, formula, CellValue::Blank)
    }
    /// Store a formula with an explicitly supplied saved result, preserving its style.
    /// The saved result is not checked against the formula or used to skip evaluation.
    pub fn set_formula_with_cached_value(
        &mut self,
        address: CellAddress,
        formula: impl Into<String>,
        cached: CellValue,
    ) -> Result<(), ModelError> {
        address.validate()?;
        let formula = normalize_formula(formula.into())?;
        self.clear_spill_cache(address);
        self.spill_cache.remove(&address);
        self.rows.insert(address.row);
        if self.get(address).is_none() {
            self.cells.insert(address, Cell::default());
        }
        let cell = self
            .cells
            .get_mut(&address)
            .ok_or_else(|| ModelError("cell insertion failed".into()))?;
        cell.formula = Some(formula);
        cell.value = cached;
        Ok(())
    }
    /// Remove a stored cell and its derived spill cache, returning the previous cell.
    pub fn remove(&mut self, address: CellAddress) -> Option<Cell> {
        self.clear_spill_cache(address);
        self.spill_cache.remove(&address);
        self.cells.remove(&address)
    }
}
/// The calendar used to interpret numbers as spreadsheet dates.
///
/// The default 1900 system includes Excel's fictional leap-day slot; the 1904
/// system uses a different origin. Dates remain numeric cell values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum DateSystem {
    /// 1900 date system including Excel's fictional leap-day serial.
    #[default]
    Excel1900,
    /// 1904 date system.
    Excel1904,
}
/// A named formula, optionally scoped to one sheet by its zero-based index.
///
/// Despite the name, its formula may return a scalar or expression as well as a range.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NamedRange {
    /// Defined-name spelling used for scoped lookup.
    pub name: String,
    /// Normalized expression backing the defined name.
    pub formula: String,
    /// Local sheet index, or None for workbook scope.
    pub sheet_scope: Option<usize>,
}
/// File representation used when importing workbook source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileFormat {
    /// OOXML spreadsheet package.
    Xlsx,
    /// BIFF binary spreadsheet.
    Xls,
}

/// The original file format and warnings from importing a workbook.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileMetadata {
    /// Original spreadsheet representation.
    pub format: FileFormat,
    /// Features omitted or altered by the importer.
    pub import_warnings: Vec<ImportDiagnostic>,
}

/// Category of information lost or altered during spreadsheet import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ImportDiagnosticKind {
    /// Source features outside the workbook model are omitted.
    OmittedFeatures,
    /// Name identity could not be compared or normalized completely.
    IdentifierIdentity,
    /// External relationships are neither followed nor preserved.
    ExternalLink,
    /// A source representation was reduced or recovered through backend limitations.
    RepresentationLoss,
}
/// A feature changed or omitted while importing a workbook.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ImportDiagnostic {
    /// Category for matching warnings in application code.
    pub kind: ImportDiagnosticKind,
    /// Source-specific details explaining the loss.
    pub detail: String,
}
impl std::fmt::Display for ImportDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.detail)
    }
}

/// Editable workbook data: ordered sheets, named formulas, and a date system.
///
/// This model stores formulas and saved results but does not evaluate them.
/// Use `poirot_eval` for calculation and `poirot_io` for file import/export.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Workbook {
    /// Optional provenance and diagnostics retained from file import.
    pub file_metadata: Option<FileMetadata>,
    sheets: Vec<Worksheet>,
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
    pub fn import_sheet(&mut self, name: impl Into<String>) -> Result<&mut Worksheet, ModelError> {
        let name = name.into();
        if self.sheet_index_by_name(&name).is_some() {
            return Err(ModelError(format!("duplicate worksheet name: {name}")));
        }
        self.sheets.push(Worksheet {
            name,
            presentation: SheetPresentation::default(),
            cells: cell_pages::CellStore::default(),
            rows: std::collections::BTreeSet::new(),
            visibility: BTreeMap::new(),
            spill_cache: BTreeMap::new(),
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
        if self
            .try_sheet_index_by_name(sheet.name())
            .map_err(|e| ModelError(e.to_string()))?
            .is_some()
        {
            return Err(ModelError(format!(
                "duplicate worksheet name: {}",
                sheet.name()
            )));
        }
        self.sheets.push(sheet);
        self.sheets
            .last_mut()
            .ok_or_else(|| ModelError("worksheet insertion failed".into()))
    }
    /// Metadata-only rename. Use a higher-layer edit planner to preserve formula references.
    pub fn rename_sheet(
        &mut self,
        index: usize,
        name: impl Into<String>,
    ) -> Result<(), ModelError> {
        let name = name.into();
        validate_sheet_name(&name)?;
        for (i, sheet) in self.sheets.iter().enumerate() {
            if i != index
                && identifiers::checked_matches(sheet.name(), &name)
                    .map_err(|e| ModelError(e.to_string()))?
            {
                return Err(ModelError("duplicate worksheet name".into()));
            }
        }
        let sheet = self
            .sheet_mut(index)
            .ok_or_else(|| ModelError("worksheet index out of bounds".into()))?;
        sheet.name = name;
        Ok(())
    }
    /// Remove worksheet metadata and its local defined names; adjust later scopes.
    /// Callers must rewrite surviving formulas before publishing this mutation.
    pub fn remove_sheet(&mut self, index: usize) -> Result<Worksheet, ModelError> {
        if index >= self.sheets.len() {
            return Err(ModelError("worksheet index out of bounds".into()));
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
            return Err(ModelError("worksheet index out of bounds".into()));
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
        if self
            .try_sheet_index_by_name(&name)
            .map_err(|e| ModelError(e.to_string()))?
            .is_some()
        {
            return Err(ModelError("duplicate worksheet name".into()));
        }
        let mut copy = self
            .sheet(index)
            .ok_or_else(|| ModelError("worksheet index out of bounds".into()))?
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
            return Err(ModelError("invalid defined name".into()));
        }
        if sheet_scope.is_some_and(|i| i >= self.sheets.len()) {
            return Err(ModelError(
                "name scope worksheet index out of bounds".into(),
            ));
        }
        for existing in self
            .named_ranges
            .iter()
            .filter(|n| n.sheet_scope == sheet_scope)
        {
            if identifiers::checked_matches(&existing.name, &name)
                .map_err(|e| ModelError(e.to_string()))?
            {
                return Err(ModelError("duplicate defined name".into()));
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn addresses_round_trip_and_validate() {
        for (text, row, col) in [
            ("A1", 0, 0),
            ("Z9", 8, 25),
            ("AA1", 0, 26),
            ("XFD1048576", MAX_ROWS - 1, MAX_COLUMNS - 1),
        ] {
            let a: CellAddress = text.parse().unwrap();
            assert_eq!(a, CellAddress::new(row, col).unwrap());
            assert_eq!(a.to_string(), text);
        }
        assert_eq!("$a$1".parse::<CellAddress>().unwrap().to_string(), "A1");
        for text in [
            "A0",
            "XFE1",
            "A1048577",
            "1A",
            "A",
            "A-1",
            "A1junk",
            "💥1",
            "A42949672960",
        ] {
            assert!(text.parse::<CellAddress>().is_err(), "{text}");
        }
    }
    #[test]
    fn sheet_names_and_lookup() {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        assert!(w.add_sheet("DATA").is_err());
        assert!(w.sheet_by_name("data").is_some());
        for name in ["", "bad/name", "'quoted", "a:b"] {
            assert!(w.add_sheet(name).is_err());
        }
        assert!(w.add_sheet("😀".repeat(16)).is_err());
        w.rename_sheet(0, "Results").unwrap();
        assert_eq!(w.sheet(0).unwrap().name(), "Results");
        assert!(w.rename_sheet(4, "Other").is_err());
    }
    #[test]
    fn formulas_preserve_cache_and_style() {
        let mut s = Worksheet::new("Sheet1").unwrap();
        let a = "A1".parse().unwrap();
        s.set_formula_with_cached_value(a, " = SUM(B1:B2) ", CellValue::Number(3.0))
            .unwrap();
        let c = s.get(a).unwrap();
        assert_eq!(c.formula.as_deref(), Some("SUM(B1:B2)"));
        assert_eq!(c.value, CellValue::Number(3.0));
        assert!(s.set_formula(a, "=").is_err());
        assert_eq!(s.get(a).unwrap().value, CellValue::Number(3.0));
        s.get_mut(a).unwrap().style.number_format = Some("0.00".into());
        s.set_value(a, true).unwrap();
        assert_eq!(s.get(a).unwrap().formula, None);
        assert_eq!(
            s.get(a).unwrap().style.number_format.as_deref(),
            Some("0.00")
        );
        assert!(
            s.set_value(
                CellAddress {
                    row: MAX_ROWS,
                    col: 0
                },
                1.0
            )
            .is_err()
        );
    }
    #[test]
    fn errors_round_trip() {
        for error in [
            CellError::Null,
            CellError::Div0,
            CellError::Value,
            CellError::Ref,
            CellError::Name,
            CellError::Num,
            CellError::NA,
            CellError::GettingData,
            CellError::Spill,
            CellError::Calc,
        ] {
            assert_eq!(error.to_string().parse::<CellError>().unwrap(), error);
        }
        assert!("nope".parse::<CellError>().is_err());
    }
    #[test]
    fn names_resolve_local_before_global() {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        w.define_name("Rate", "=0.1", None).unwrap();
        w.define_name("Rate", "0.2", Some(0)).unwrap();
        assert_eq!(w.defined_name("RATE", 0).unwrap().formula, "0.2");
        assert!(w.define_name("A1", "1", None).is_err());
        assert!(w.define_name("Rate", "1", None).is_err());
        assert!(w.define_name("Other", "1", Some(1)).is_err());
    }
}

/// Serialization of workbook data and stored formula results.
pub mod snapshot;
#[cfg(test)]
mod scalar_boundary_tests {
    use super::*;
    #[test]
    fn formula_cache_and_source_remain_separate() {
        let mut sheet = crate::Worksheet::new("Data").unwrap();
        let address = crate::CellAddress::new(0, 0).unwrap();
        let value = CellValue::from(ExcelText::from_units(vec![0xd83d]));
        sheet
            .set_formula_with_cached_value(address, "LEFT(\"😀\",1)", value.clone())
            .unwrap();
        assert_eq!(sheet.get(address).unwrap().value, value);
        assert_eq!(
            sheet.get(address).unwrap().formula.as_deref(),
            Some("LEFT(\"😀\",1)")
        );
        sheet.set_value(address, "😀").unwrap();
        assert!(matches!(
            sheet.get(address).unwrap().value,
            CellValue::Text(_)
        ));
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

/// Borrowed cell reads shared by worksheets and paged stores with arbitrary payload/layout.
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

/// Derived child data retains the prior source cell so clearing a footprint never
/// deletes author-authored blank/style metadata or promotes cached data to input.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SpillCell {
    /// Anchor coordinate that owns the derived child.
    pub owner: CellAddress,
    /// Original authored child cell restored when derived cache is cleared.
    pub source: Option<Cell>,
}
impl Worksheet {
    /// Borrow authored content, excluding a derived child's saved spill value.
    pub fn source_cell(&self, address: CellAddress) -> Option<&Cell> {
        self.spill_cache
            .get(&address)
            .map_or_else(|| self.get(address), |spill| spill.source.as_ref())
    }
    /// Return the anchor owning a derived cached child.
    pub fn spill_owner(&self, address: CellAddress) -> Option<CellAddress> {
        self.spill_cache.get(&address).map(|s| s.owner)
    }
    /// Remove one anchor's derived values and restore original authored cells.
    pub fn clear_spill_cache(&mut self, owner: CellAddress) {
        let children = self
            .spill_cache
            .iter()
            .filter(|(_, s)| s.owner == owner)
            .map(|(at, _)| *at)
            .collect::<Vec<_>>();
        for child in children {
            let Some(cached) = self.spill_cache.remove(&child) else {
                continue;
            };
            match cached.source {
                Some(source) => {
                    self.cells.insert(child, source);
                }
                None => {
                    self.cells.remove(&child);
                }
            }
        }
    }
    /// Validate formula ownership and save a derived child without losing authored metadata.
    pub fn set_spill_cached_value(
        &mut self,
        address: CellAddress,
        owner: CellAddress,
        expected_formula: &str,
        value: CellValue,
    ) -> Result<(), ModelError> {
        address.validate()?;
        owner.validate()?;
        if address == owner
            || self.get(owner).and_then(|c| c.formula.as_deref()) != Some(expected_formula)
        {
            return Err(ModelError(
                "spill cached-value anchor source changed".into(),
            ));
        }
        let source = self.source_cell(address);
        if source.is_some_and(|cell| cell.formula.is_some() || cell.value != CellValue::Blank) {
            return Err(ModelError(
                "spill cache cannot overwrite an authored cell".into(),
            ));
        }
        if self
            .spill_cache
            .get(&address)
            .is_some_and(|s| s.owner != owner)
        {
            return Err(ModelError("spill cache ownership conflict".into()));
        }
        let source = source.cloned();
        let style = source.as_ref().map(|c| c.style.clone()).unwrap_or_default();
        self.rows.insert(address.row);
        self.cells.insert(
            address,
            Cell {
                value,
                formula: None,
                style,
            },
        );
        self.spill_cache
            .insert(address, SpillCell { owner, source });
        Ok(())
    }
}

#[cfg(test)]
mod sheet_management_tests {
    use super::*;
    #[test]
    fn reorder_remaps_scopes_and_duplicate_retains_typed_sources_and_presentation() {
        let mut w = Workbook::new();
        for name in ["A", "B", "C"] {
            w.add_sheet(name).unwrap();
        }
        w.define_name("local", "A1", Some(0)).unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), ExcelText::from_units(vec![0xd800]))
            .unwrap();
        w.sheet_mut(0).unwrap().set_presentation(SheetPresentation {
            visibility: SheetVisibility::VeryHidden,
            tab_color: Some([1, 2, 3]),
        });
        let copy = w.duplicate_sheet(0, "Copy").unwrap();
        assert_eq!(copy, 3);
        assert_eq!(
            w.sheet(copy).unwrap().presentation(),
            SheetPresentation {
                visibility: SheetVisibility::Visible,
                tab_color: Some([1, 2, 3])
            }
        );
        assert_eq!(
            w.sheet(copy).unwrap().get("A1".parse().unwrap()),
            w.sheet(0).unwrap().get("A1".parse().unwrap())
        );
        assert!(w.named_ranges.iter().any(|n| n.sheet_scope == Some(3)));
        w.move_sheet(0, 3).unwrap();
        assert_eq!(
            w.sheets().iter().map(|s| s.name()).collect::<Vec<_>>(),
            ["B", "C", "Copy", "A"]
        );
        assert_eq!(
            w.named_ranges
                .iter()
                .map(|n| n.sheet_scope)
                .collect::<Vec<_>>(),
            [Some(3), Some(2)]
        );
        w.move_sheet(3, 0).unwrap();
        let original = w.clone();
        assert!(w.move_sheet(0, 99).is_err());
        assert!(w.duplicate_sheet(0, "a").is_err());
        assert!(w.duplicate_sheet(0, "bad/name").is_err());
        assert_eq!(w, original);
    }
}
