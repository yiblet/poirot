//! Sparse worksheets, presentation, and saved spill ownership.
use crate::model::cell::normalize_formula;
use crate::model::{
    Cell, CellAddress, CellStyle, CellValue, RowVisibility, WorksheetCells, cell_pages,
};
use poirot_io::{ModelError, SheetPresentation};
use std::collections::BTreeMap;

/// Validate sheet-name spelling before storing it in the workbook.
pub(super) fn validate_sheet_name(name: &str) -> Result<(), ModelError> {
    if name.is_empty()
        || name.encode_utf16().count() > 31
        || name
            .chars()
            .any(|c| matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']' | '\0' | '\u{3}'))
        || name.starts_with('\'')
        || name.ends_with('\'')
    {
        Err(ModelError::InvalidWorksheetName { name: name.into() })
    } else {
        Ok(())
    }
}
/// A named worksheet with stored cells and explicit row records.
///
/// Valid missing coordinates allocate no cell; [`Worksheet::get`] returns `Ok(None)` for them.
/// Setting a value clears its formula. Setting a formula starts with a blank saved
/// result; use [`Worksheet::set_formula_with_cached_value`] to supply one explicitly.
///
/// ```rust
/// use poirot::model::{CellValue, SheetPresentation, SheetVisibility, Worksheet};
///
/// let mut sheet = Worksheet::new("Inputs")?;
/// sheet.set_value("A1", CellValue::Blank)?;
/// sheet.set_formula("B1", "A1+1")?;
/// sheet.set_presentation(SheetPresentation {
///     visibility: SheetVisibility::Hidden,
///     tab_color: Some([30, 120, 200]),
/// });
/// assert!(sheet.get("A1")?.is_some());
/// assert!(sheet.get("A2")?.is_none());
/// assert_eq!(sheet.get("B1")?.ok_or("missing formula")?.value, CellValue::Blank);
/// assert_eq!(sheet.presentation().visibility, SheetVisibility::Hidden);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Worksheet {
    pub(super) name: String,
    pub(super) presentation: SheetPresentation,
    pub(super) cells: cell_pages::CellStore,
    pub(super) rows: std::collections::BTreeSet<u32>,
    pub(super) visibility: BTreeMap<u32, RowVisibility>,
    pub(super) spill_cache: BTreeMap<CellAddress, SpillCell>,
    pub(super) spill_extents: BTreeMap<CellAddress, SavedSpillExtent>,
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
            spill_extents: BTreeMap::new(),
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
    /// Read a stored cell, or return `Ok(None)` if the position has no cell.
    ///
    /// Accepts worksheet-local A1 text, a [`CellAddress`], or zero-based `(row, column)`
    /// integers. Invalid notation, negative indices and coordinates outside worksheet
    /// bounds return an error. Reads never allocate a missing cell.
    ///
    /// An explicitly stored blank returns `Ok(Some(_))`, just like any other cell.
    pub fn get<A>(&self, address: A) -> Result<Option<&Cell>, ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        Ok(self.get_validated(address))
    }
    /// Borrow storage using a typed coordinate, without conversion or bounds validation.
    pub(crate) fn get_validated(&self, address: CellAddress) -> Option<&Cell> {
        self.cells.get(&address)
    }
    /// Mutably borrow a stored cell, rejecting invalid coordinates.
    ///
    /// Direct edits bypass formula normalization, shared-provenance detachment,
    /// and spill cleanup. Use [`Self::set_value`] or [`Self::set_formula`] when replacing cell contents.
    pub fn get_mut<A>(&mut self, address: A) -> Result<Option<&mut Cell>, ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        Ok(self.get_mut_validated(address))
    }
    /// Mutably borrow storage using a typed coordinate, without conversion or bounds validation.
    pub(crate) fn get_mut_validated(&mut self, address: CellAddress) -> Option<&mut Cell> {
        self.cells.get_mut(&address)
    }
    /// Replace a cell, returning the previous cell if one was stored.
    ///
    /// Validates the address and formula, then clears any conflicting spill results.
    pub fn set<A>(&mut self, address: A, cell: Cell) -> Result<Option<Cell>, ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        let mut cell = cell;
        if let Some(f) = cell.formula.take() {
            cell.formula = Some(normalize_formula(f)?);
        }
        cell.validate_shared_formula()?;
        self.clear_spill_cache(address);
        if let Some(child) = self.spill_cache.remove(&address) {
            self.spill_extents.remove(&child.owner);
        }
        self.rows.insert(address.row);
        Ok(self.cells.insert(address, cell))
    }
    /// Change display style while preserving authored content and saved spill ownership.
    pub fn set_style<A>(&mut self, address: A, style: CellStyle) -> Result<(), ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        if let Some(child) = self.spill_cache.get_mut(&address) {
            child.source.get_or_insert_with(Cell::default).style = style.clone();
        }
        if let Some(cell) = self.cells.get_mut(&address) {
            cell.style = style;
        } else {
            self.cells.insert(
                address,
                Cell {
                    style,
                    ..Default::default()
                },
            );
        }
        self.rows.insert(address.row);
        Ok(())
    }
    /// Store a literal value and clear any formula at this address, retaining its style.
    pub fn set_value<A>(
        &mut self,
        address: A,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        self.clear_spill_cache(address);
        if let Some(child) = self.spill_cache.remove(&address) {
            self.spill_extents.remove(&child.owner);
        }
        self.rows.insert(address.row);
        if self.get_validated(address).is_none() {
            self.cells.insert(address, Cell::default());
        }
        let cell = self
            .cells
            .get_mut(&address)
            .ok_or(ModelError::CellInsertionMissing { address })?;
        cell.value = value.into();
        cell.formula = None;
        cell.shared_formula = None;
        Ok(())
    }
    /// Store a formula and clear its saved result, preserving the cell's style.
    /// A leading `=` is optional. This does not evaluate the formula.
    pub fn set_formula<A>(
        &mut self,
        address: A,
        formula: impl Into<String>,
    ) -> Result<(), ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        self.set_formula_with_cached_value(address, formula, CellValue::Blank)
    }
    /// Store a formula with an explicitly supplied saved result, preserving its style.
    /// The saved result is not checked against the formula or used to skip evaluation.
    pub fn set_formula_with_cached_value<A>(
        &mut self,
        address: A,
        formula: impl Into<String>,
        cached: CellValue,
    ) -> Result<(), ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        let formula = normalize_formula(formula.into())?;
        self.clear_spill_cache(address);
        if let Some(child) = self.spill_cache.remove(&address) {
            self.spill_extents.remove(&child.owner);
        }
        self.rows.insert(address.row);
        if self.get_validated(address).is_none() {
            self.cells.insert(address, Cell::default());
        }
        let cell = self
            .cells
            .get_mut(&address)
            .ok_or(ModelError::CellInsertionMissing { address })?;
        cell.formula = Some(formula);
        cell.shared_formula = None;
        cell.value = cached;
        Ok(())
    }
    /// Remove a stored cell and its derived spill cache, rejecting invalid coordinates.
    pub fn remove<A>(&mut self, address: A) -> Result<Option<Cell>, ModelError>
    where
        A: TryInto<CellAddress>,
        A::Error: Into<ModelError>,
    {
        let address = address.try_into().map_err(Into::into)?;
        address.validate()?;
        self.clear_spill_cache(address);
        if let Some(child) = self.spill_cache.remove(&address) {
            self.spill_extents.remove(&child.owner);
        }
        Ok(self.cells.remove(&address))
    }
}
/// Saved spill rectangle tied to the anchor's exact formula text.
/// It preserves declared geometry even when cached child values are incomplete and
/// prevents that geometry from being reused for a different formula.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(in crate::model) struct SavedSpillExtent {
    pub(super) last: CellAddress,
    pub(super) formula: String,
}

/// Derived child data retains the prior source cell so clearing a footprint never
/// deletes author-authored blank/style metadata or promotes cached data to input.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::model) struct SpillCell {
    /// Anchor coordinate that owns the derived child.
    pub owner: CellAddress,
    /// Original authored child cell restored when derived cache is cleared.
    pub source: Option<Cell>,
}
impl Worksheet {
    /// Iterate valid saved anchor extents, including incomplete or one-cell footprints.
    pub fn spill_extents(&self) -> impl Iterator<Item = (CellAddress, CellAddress)> + '_ {
        self.spill_extents.iter().filter_map(|(anchor, extent)| {
            (self
                .get_validated(*anchor)
                .and_then(|cell| cell.formula.as_deref())
                == Some(extent.formula.as_str()))
            .then_some((*anchor, extent.last))
        })
    }
    /// Read declared saved geometry only while its exact anchor formula still matches.
    pub fn spill_extent(&self, anchor: CellAddress) -> Option<CellAddress> {
        let extent = self.spill_extents.get(&anchor)?;
        (self.get_validated(anchor)?.formula.as_deref()? == extent.formula).then_some(extent.last)
    }
    /// Save declared geometry without asserting that every cached child is present.
    pub fn set_spill_extent(
        &mut self,
        anchor: CellAddress,
        last: CellAddress,
        expected_formula: &str,
    ) -> Result<(), ModelError> {
        anchor.validate()?;
        last.validate()?;
        if last.row < anchor.row
            || last.col < anchor.col
            || self
                .get_validated(anchor)
                .and_then(|cell| cell.formula.as_deref())
                != Some(expected_formula)
        {
            return Err(ModelError::InvalidSpillExtent { anchor, last });
        }
        self.spill_extents.insert(
            anchor,
            SavedSpillExtent {
                last,
                formula: expected_formula.into(),
            },
        );
        Ok(())
    }
    /// Verify saved children cover a declared rectangle exactly, without expanding sparse holes.
    pub fn complete_spill_extent(&self, anchor: CellAddress) -> Option<CellAddress> {
        if self.spill_cache.contains_key(&anchor) {
            return None;
        }
        let last = self.spill_extent(anchor)?;
        let count = u64::from(last.row.checked_sub(anchor.row)?.checked_add(1)?)
            .checked_mul(u64::from(last.col.checked_sub(anchor.col)?.checked_add(1)?))?;
        let mut children = 0u64;
        for (at, spill) in &self.spill_cache {
            if spill.owner != anchor {
                continue;
            }
            if *at == anchor
                || at.row < anchor.row
                || at.row > last.row
                || at.col < anchor.col
                || at.col > last.col
                || self
                    .get_validated(*at)
                    .is_none_or(|cell| cell.formula.is_some())
            {
                return None;
            }
            children = children.checked_add(1)?;
        }
        (children.checked_add(1)? == count).then_some(last)
    }
    /// Borrow authored content, excluding a derived child's saved spill value.
    pub fn source_cell(&self, address: CellAddress) -> Option<&Cell> {
        self.spill_cache.get(&address).map_or_else(
            || self.get_validated(address),
            |spill| spill.source.as_ref(),
        )
    }
    /// Iterate derived cached children and their source anchors without visiting authored cells.
    pub fn spill_children(&self) -> impl Iterator<Item = (CellAddress, CellAddress)> + '_ {
        self.spill_cache
            .iter()
            .map(|(address, spill)| (*address, spill.owner))
    }
    /// Return the saved anchor owning a derived cached child, excluding anchors themselves.
    /// This source metadata does not inspect or evaluate current formula results.
    pub fn spill_owner(&self, address: CellAddress) -> Option<CellAddress> {
        self.spill_cache.get(&address).map(|s| s.owner)
    }
    /// Remove one anchor's derived values and restore original authored cells.
    pub fn clear_spill_cache(&mut self, owner: CellAddress) {
        self.spill_extents.remove(&owner);
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
            || address.row < owner.row
            || address.col < owner.col
            || self.get_validated(owner).and_then(|c| c.formula.as_deref())
                != Some(expected_formula)
        {
            return Err(ModelError::InvalidSpillCacheAnchor { address, owner });
        }
        let source = self.source_cell(address);
        if source.is_some_and(|cell| cell.formula.is_some() || cell.value != CellValue::Blank) {
            return Err(ModelError::SpillCacheAuthoredCell { address });
        }
        if self
            .spill_cache
            .get(&address)
            .is_some_and(|s| s.owner != owner)
        {
            return Err(ModelError::SpillCacheOwnershipConflict { address });
        }
        let source = source.cloned();
        let style = source.as_ref().map(|c| c.style.clone()).unwrap_or_default();
        self.rows.insert(address.row);
        self.cells.insert(
            address,
            Cell {
                value,
                formula: None,
                shared_formula: None,
                style,
            },
        );
        self.spill_cache
            .insert(address, SpillCell { owner, source });
        Ok(())
    }
}
