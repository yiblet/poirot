//! Ordered worksheets, defined names, and workbook source metadata.
use crate::model::cell::normalize_formula;
use crate::model::worksheet::validate_sheet_name;
use crate::model::{
    CellAddress, FileMetadata, ModelError, SheetPresentation, SheetVisibility, StyleContext,
    Worksheet, cell_pages, identifiers, snapshot,
};
use poirot_io::NamedRange;
use std::collections::BTreeMap;

use poirot_functions::scalar::DateSystem;
/// Editable workbook data: ordered sheets, named formulas, and a date system.
///
/// This model stores formulas and saved results but does not evaluate them.
/// Use `poirot::eval` for calculation and `poirot_io` for file import/export.
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
    /// Metadata-only rename. Use `poirot::Workbook::rename_sheet` to preserve formula references.
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
