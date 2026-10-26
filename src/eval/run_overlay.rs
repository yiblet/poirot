//! Immutable run assignments and borrowed effective cell/membership reads.
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{Cell, CellAddress, CellValue, ModelError, Workbook};
use std::collections::{BTreeMap, BTreeSet};
/// One sheet's exclusive edits, kept sparse even for isolated deletions.
#[derive(Default)]
pub(in crate::eval) struct InputCells {
    edits: BTreeMap<CellAddress, CellEdit>,
    /// Derived coordinate-only column order; edit meaning lives exclusively in `edits`.
    columns: BTreeSet<(u32, u32)>,
}
impl InputCells {
    /// Validate and replace one edit without allocating a dense cell page.
    pub(in crate::eval) fn set(
        &mut self,
        address: CellAddress,
        edit: CellEdit,
    ) -> Result<Option<CellEdit>, ModelError> {
        address.validate()?;
        let old = self.edits.insert(address, edit);
        if old.is_none() {
            self.columns.insert((address.col, address.row));
        }
        Ok(old)
    }
    /// Store content, reusing its existing allocation; report whether an edit existed.
    pub(in crate::eval) fn set_cell(
        &mut self,
        address: CellAddress,
        cell: Cell,
        style_only: bool,
    ) -> Result<bool, ModelError> {
        address.validate()?;
        match self.edits.entry(address) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let previous = std::mem::replace(entry.get_mut(), CellEdit::Delete);
                let cell = match previous {
                    CellEdit::Replace(mut stored) | CellEdit::StyleOnly(mut stored) => {
                        *stored = cell;
                        stored
                    }
                    // SAFETY: Cell has a fixed-size Rust layout, so this box cannot overflow its allocation size.
                    CellEdit::Delete => Box::new(cell),
                };
                *entry.get_mut() = CellEdit::with_cell(cell, style_only);
                Ok(true)
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                // SAFETY: Cell has a fixed-size Rust layout, so this box cannot overflow its allocation size.
                entry.insert(CellEdit::with_cell(Box::new(cell), style_only));
                self.columns.insert((address.col, address.row));
                Ok(false)
            }
        }
    }
    /// Borrow an edit at its authored coordinate.
    pub(in crate::eval) fn get(&self, address: &CellAddress) -> Option<&CellEdit> {
        self.edits.get(address)
    }
    /// Mutably borrow an existing edit.
    pub(in crate::eval) fn get_mut(&mut self, address: &CellAddress) -> Option<&mut CellEdit> {
        self.edits.get_mut(address)
    }
    /// Remove one edit, restoring baseline inheritance.
    pub(in crate::eval) fn remove(&mut self, address: &CellAddress) -> Option<CellEdit> {
        let old = self.edits.remove(address);
        if old.is_some() {
            self.columns.remove(&(address.col, address.row));
        }
        old
    }
    /// Borrow the ordered edits for sparse traversal.
    pub(in crate::eval) fn cells(&self) -> &BTreeMap<CellAddress, CellEdit> {
        &self.edits
    }
    /// Borrow a bounded edit interval; reversed bounds produce an empty cursor.
    fn range(&self, range: std::ops::RangeInclusive<CellAddress>) -> InputRange<'_> {
        if range.start() > range.end() {
            // SAFETY: Equal included/excluded bounds form a valid empty interval.
            return self.edits.range(*range.start()..*range.start());
        }
        // SAFETY: The guard above excludes reversed inclusive bounds.
        self.edits.range(range)
    }
}
/// Borrowed coordinate-ordered view of overlay edits within a rectangle.
type InputRange<'a> = std::collections::btree_map::Range<'a, CellAddress, CellEdit>;

/// One authored mutation; absent entries inherit baseline content and ownership.
#[derive(Clone)]
pub(in crate::eval) enum CellEdit {
    /// Explicit content replacement, including an authored blank.
    Replace(Box<Cell>),
    /// Changed style while baseline content and saved ownership remain authoritative.
    StyleOnly(Box<Cell>),
    /// Hidden baseline source, distinct from an authored blank.
    Delete,
}
impl CellEdit {
    /// Attach the binary baseline-preserving style property to stored content.
    fn with_cell(cell: Box<Cell>, style_only: bool) -> Self {
        if style_only {
            Self::StyleOnly(cell)
        } else {
            Self::Replace(cell)
        }
    }
    /// Borrow content when this edit stores a cell.
    pub(in crate::eval) fn cell(&self) -> Option<&Cell> {
        match self {
            Self::Replace(cell) | Self::StyleOnly(cell) => Some(cell),
            Self::Delete => None,
        }
    }
    /// Whether this edit preserves baseline content and ownership.
    pub(in crate::eval) fn is_style_only(&self) -> bool {
        matches!(self, Self::StyleOnly(_))
    }
}

/// Sparse scenario assignments prepared for one shared workbook source.
/// The source token prevents applying a batch to another baseline, while unique
/// resolved coordinates let evaluation merge edits without ambiguous duplicates.
pub struct RunInputs {
    /// Exclusive source edits in sparse coordinate order.
    pub(in crate::eval) sheets: BTreeMap<usize, InputCells>,
    /// Source identity token binding assignments to their shared workbook owner.
    pub(in crate::eval) source: std::sync::Arc<()>,
}
impl RunInputs {
    /// Create empty sparse inputs with a fresh source token.
    pub(in crate::eval) fn empty() -> Self {
        Self {
            sheets: BTreeMap::new(),
            source: std::sync::Arc::new(()),
        }
    }
    /// Validate unique literal assignments and preserve baseline cell styles.
    pub(in crate::eval) fn new(
        workbook: &Workbook,
        assignments: impl IntoIterator<Item = (usize, CellAddress, CellValue)>,
        max_assignments: usize,
        source: std::sync::Arc<()>,
    ) -> Result<Self, ModelError> {
        let mut inputs = Self {
            sheets: BTreeMap::new(),
            source,
        };
        let mut count = 0usize;
        for (sheet, address, value) in assignments {
            address.validate()?;
            let worksheet = workbook
                .sheet(sheet)
                .ok_or_else(|| ModelError("input worksheet index out of bounds".into()))?;
            count = count
                .checked_add(1)
                .ok_or_else(|| ModelError("input assignment count overflow".into()))?;
            if count > max_assignments {
                return Err(ModelError("input assignment budget exceeded".into()));
            }
            let mut cell = Cell::new(value);
            if let Some(original) = worksheet.get(address) {
                cell.style = original.style.clone();
            }
            if inputs
                .sheets
                .entry(sheet)
                .or_default()
                .set_cell(address, cell, false)?
            {
                return Err(ModelError("duplicate input assignment".into()));
            }
        }
        Ok(inputs)
    }
    /// Read an explicit override without falling back to the baseline.
    pub fn cell(&self, sheet: usize, address: CellAddress) -> Option<&Cell> {
        self.edit(sheet, address)?.cell()
    }
    /// Read an authored edit without consulting baseline content.
    pub(in crate::eval) fn edit(&self, sheet: usize, address: CellAddress) -> Option<&CellEdit> {
        self.sheets.get(&sheet)?.get(&address)
    }
    /// Whether a coordinate has an explicit deletion marker.
    pub(in crate::eval) fn is_deleted(&self, sheet: usize, address: CellAddress) -> bool {
        matches!(self.edit(sheet, address), Some(CellEdit::Delete))
    }
    /// Traverse all edits once in sheet and coordinate order.
    pub(in crate::eval) fn edits(&self) -> impl Iterator<Item = (usize, CellAddress, &CellEdit)> {
        self.sheets.iter().flat_map(|(&sheet, cells)| {
            cells
                .cells()
                .iter()
                .map(move |(&address, edit)| (sheet, address, edit))
        })
    }
}

/// Read view combining a baseline workbook with sparse scenario inputs.
/// Evaluation and workbook projection see the same overridden cells while sheet
/// metadata remains owned by the baseline.
#[derive(Clone, Copy)]
pub struct EffectiveWorkbook<'a> {
    workbook: &'a Workbook,
    inputs: Option<&'a RunInputs>,
}
impl<'a> EffectiveWorkbook<'a> {
    /// Borrow a baseline and optional sparse overrides through one read port.
    pub fn new(workbook: &'a Workbook, inputs: Option<&'a RunInputs>) -> Self {
        Self { workbook, inputs }
    }
    /// Read effective authored content, respecting overrides and deletion markers.
    pub fn cell(self, sheet: usize, address: CellAddress) -> Option<&'a Cell> {
        match self.inputs.and_then(|inputs| inputs.edit(sheet, address)) {
            Some(edit) => edit.cell(),
            None => self.workbook.sheet(sheet)?.source_cell(address),
        }
    }
    /// Traverse effective stored cells in row-major order within a rectangle.
    pub fn stored(self, area: RangeMembership) -> Result<EffectiveStoredCells<'a>, ModelError> {
        self.membership(area, false)
    }
    /// Traverse effective stored cells in reverse row-major order.
    pub fn stored_reverse(
        self,
        area: RangeMembership,
    ) -> Result<EffectiveStoredCells<'a>, ModelError> {
        self.membership(area, true)
    }
    fn membership(
        self,
        area: RangeMembership,
        reverse: bool,
    ) -> Result<EffectiveStoredCells<'a>, ModelError> {
        RangeMembership::new(area.sheet, area.first, area.last)?;
        let baseline_sheet = self
            .workbook
            .sheet(area.sheet)
            .ok_or_else(|| ModelError("query worksheet index out of bounds".into()))?;
        let baseline = baseline_sheet.cells();
        let overlay = self
            .inputs
            .and_then(|inputs| inputs.sheets.get(&area.sheet));
        // Full-width rectangles are contiguous in CellAddress order. Keep the
        // existing merge cursors open across rows instead of locating each row
        // and reopening its bounded source ranges.
        let contiguous = area.first.col == 0 && area.last.col == crate::model::MAX_COLUMNS - 1;
        let mut base_row =
            contiguous.then(|| OverlayCells::Base(baseline).range(area.first..=area.last));
        let mut input_row = overlay
            .filter(|_| contiguous)
            .map(|cells| OverlayCells::Input(cells).range(area.first..=area.last));
        let base_next = advance(&mut base_row, reverse);
        let input_next = advance(&mut input_row, reverse);
        Ok(EffectiveStoredCells {
            baseline,
            baseline_sheet,
            overlay,
            area,
            reverse,
            next_row: (!contiguous).then_some(if reverse {
                area.last.row
            } else {
                area.first.row
            }),
            base_row,
            input_row,
            base_next,
            input_next,
            column: (!reverse && area.first.col == area.last.col)
                .then(|| ColumnTraversal::new(baseline, overlay, area)),
            counters: QueryCounters {
                // Each contiguous source interval replaces the outer occupied-
                // row lookup and per-row bounded reopen operations below.
                row_seeks: if contiguous {
                    // SAFETY: Boolean conversion is zero or one, so this sum is at most two.
                    1 + u64::from(overlay.is_some())
                } else {
                    0
                },
                ..QueryCounters::default()
            },
        })
    }
}

/// Iterates stored cells after merging baseline content with scenario edits and deletions.
/// Row and column queries visit the effective sparse union without materializing a
/// second membership index; counters include source seeks and prefetched lookups.
pub struct EffectiveStoredCells<'a> {
    baseline: crate::model::WorksheetCells<'a>,
    baseline_sheet: &'a crate::model::Worksheet,
    overlay: Option<&'a InputCells>,
    area: RangeMembership,
    reverse: bool,
    next_row: Option<u32>,
    base_row: Option<OverlayRange<'a>>,
    input_row: Option<OverlayRange<'a>>,
    base_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    input_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    column: Option<ColumnTraversal<'a>>,
    counters: QueryCounters,
}
impl EffectiveStoredCells<'_> {
    /// Return sparse traversal counters accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }
}
/// Borrowed source selected by a merge cursor before tombstone filtering.
#[derive(Clone, Copy)]
enum StoredSource<'a> {
    Baseline(&'a Cell),
    Edit(&'a CellEdit),
}
impl<'a> StoredSource<'a> {
    fn cell(self) -> Option<&'a Cell> {
        match self {
            Self::Baseline(cell) => Some(cell),
            Self::Edit(edit) => edit.cell(),
        }
    }
}

/// Merged base/overlay source iterator preserving edit precedence.
#[derive(Clone, Copy)]
enum OverlayCells<'a> {
    Base(crate::model::WorksheetCells<'a>),
    Input(&'a InputCells),
}
impl<'a> OverlayCells<'a> {
    fn range(self, range: std::ops::RangeInclusive<CellAddress>) -> OverlayRange<'a> {
        match self {
            Self::Base(c) => OverlayRange::Base(c.range(range)),
            Self::Input(c) => OverlayRange::Input(c.range(range)),
        }
    }
}
/// Merged bounded base/overlay iterator used by sparse range consumers.
enum OverlayRange<'a> {
    Base(crate::model::CellRange<'a>),
    Input(InputRange<'a>),
}
impl<'a> Iterator for OverlayRange<'a> {
    /// Borrowed effective coordinate and cell after overlay precedence.
    type Item = (&'a CellAddress, StoredSource<'a>);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r
                .next()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell))),
            Self::Input(r) => r.next().map(|(at, edit)| (at, StoredSource::Edit(edit))),
        }
    }
}
impl DoubleEndedIterator for OverlayRange<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r
                .next_back()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell))),
            Self::Input(r) => r
                .next_back()
                .map(|(at, edit)| (at, StoredSource::Edit(edit))),
        }
    }
}
fn advance<'a>(
    range: &mut Option<OverlayRange<'a>>,
    reverse: bool,
) -> Option<(&'a CellAddress, StoredSource<'a>)> {
    let range = range.as_mut()?;
    if reverse {
        range.next_back()
    } else {
        range.next()
    }
}
/// Column-ordered key cursor borrowing authoritative edits without scanning other columns.
struct InputColumn<'a> {
    range: std::collections::btree_set::Range<'a, (u32, u32)>,
    edits: &'a InputCells,
    counters: QueryCounters,
}
impl<'a> InputColumn<'a> {
    fn new(edits: &'a InputCells, area: RangeMembership) -> Self {
        let first = (area.first.col, area.first.row);
        let last = (area.last.col, area.last.row);
        let range = if first <= last {
            // SAFETY: The branch proves ordered inclusive bounds.
            edits.columns.range(first..=last)
        } else {
            // SAFETY: Equal included/excluded bounds form a valid empty interval.
            edits.columns.range(first..first)
        };
        Self {
            range,
            edits,
            counters: QueryCounters {
                row_seeks: 1,
                column_seeks: 1,
                ..Default::default()
            },
        }
    }
    fn counters(&self) -> QueryCounters {
        self.counters
    }
}
impl<'a> Iterator for InputColumn<'a> {
    type Item = (&'a CellAddress, &'a CellEdit);
    fn next(&mut self) -> Option<Self::Item> {
        for &(col, row) in self.range.by_ref() {
            self.counters.key_lookups = self.counters.key_lookups.saturating_add(1);
            if let Some(item) = self.edits.edits.get_key_value(&CellAddress { row, col }) {
                self.counters.cells_visited = self.counters.cells_visited.saturating_add(1);
                return Some(item);
            }
        }
        None
    }
}

/// Merge baseline and override column cursors without a separate accounting walk.
struct ColumnTraversal<'a> {
    base: crate::model::ColumnCells<'a>,
    input: Option<InputColumn<'a>>,
    base_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    input_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    started: bool,
    finished: bool,
}
impl<'a> ColumnTraversal<'a> {
    fn new(
        base: crate::model::WorksheetCells<'a>,
        input: Option<&'a InputCells>,
        area: RangeMembership,
    ) -> Self {
        Self {
            base: base.column(area.first.col, area.first.row, area.last.row),
            input: input.map(|cells| InputColumn::new(cells, area)),
            base_next: None,
            input_next: None,
            started: false,
            finished: false,
        }
    }
    fn next(
        &mut self,
        counters: &mut QueryCounters,
    ) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        if self.finished {
            return None;
        }
        if !self.started {
            self.started = true;
            self.base_next = self
                .base
                .next()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
            self.input_next = self
                .input
                .as_mut()
                .and_then(Iterator::next)
                .map(|(at, edit)| (at, StoredSource::Edit(edit)));
        }
        let selected = match (self.base_next, self.input_next) {
            (Some(base), Some(input)) if base.0 == input.0 => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                self.input_next = self
                    .input
                    .as_mut()
                    .and_then(Iterator::next)
                    .map(|(at, edit)| (at, StoredSource::Edit(edit)));
                Some(input)
            }
            (Some(base), Some(input)) if base.0 < input.0 => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                Some(base)
            }
            (_, Some(input)) => {
                self.input_next = self
                    .input
                    .as_mut()
                    .and_then(Iterator::next)
                    .map(|(at, edit)| (at, StoredSource::Edit(edit)));
                Some(input)
            }
            (Some(base), None) => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                Some(base)
            }
            (None, None) => None,
        };
        *counters = self.base.counters();
        if let Some(input) = &self.input {
            let input = input.counters();
            counters.row_seeks = counters.row_seeks.saturating_add(input.row_seeks);
            counters.column_seeks = counters.column_seeks.saturating_add(input.column_seeks);
            counters.cells_visited = counters.cells_visited.saturating_add(input.cells_visited);
            counters.key_lookups = counters.key_lookups.saturating_add(input.key_lookups);
        }
        self.finished = selected.is_none();
        selected
    }
}
impl<'a> Iterator for EffectiveStoredCells<'a> {
    /// Borrowed effective source cell excluding removed baseline entries.
    type Item = (&'a CellAddress, &'a Cell);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(column) = &mut self.column {
            loop {
                let item = column.next(&mut self.counters)?;
                let Some(cell) = item.1.cell() else { continue };
                if matches!(item.1, StoredSource::Edit(_))
                    || self.baseline_sheet.spill_owner(*item.0).is_none()
                {
                    return Some((item.0, cell));
                }
                if let Some(source) = self.baseline_sheet.source_cell(*item.0) {
                    return Some((item.0, source));
                }
            }
        }
        loop {
            let selected = match (self.base_next, self.input_next) {
                (Some(base), Some(input)) if base.0 == input.0 => {
                    self.base_next = advance(&mut self.base_row, self.reverse);
                    self.input_next = advance(&mut self.input_row, self.reverse);
                    Some(input)
                }
                (Some(base), Some(input)) => {
                    if (base.0 < input.0) != self.reverse {
                        self.base_next = advance(&mut self.base_row, self.reverse);
                        Some(base)
                    } else {
                        self.input_next = advance(&mut self.input_row, self.reverse);
                        Some(input)
                    }
                }
                (Some(base), None) => {
                    self.base_next = advance(&mut self.base_row, self.reverse);
                    Some(base)
                }
                (None, Some(input)) => {
                    self.input_next = advance(&mut self.input_row, self.reverse);
                    Some(input)
                }
                (None, None) => None,
            };
            if let Some(item) = selected {
                let Some(cell) = item.1.cell() else { continue };
                let item = if matches!(item.1, StoredSource::Edit(_))
                    || self.baseline_sheet.spill_owner(*item.0).is_none()
                {
                    (item.0, cell)
                } else if let Some(source) = self.baseline_sheet.source_cell(*item.0) {
                    (item.0, source)
                } else {
                    continue;
                };
                self.counters.cells_visited += 1;
                return Some(item);
            }
            let next = self.next_row?;
            self.counters.row_seeks += 1;
            let first = CellAddress {
                row: if self.reverse {
                    self.area.first.row
                } else {
                    next
                },
                col: 0,
            };
            let last = CellAddress {
                row: if self.reverse {
                    next
                } else {
                    self.area.last.row
                },
                col: crate::model::MAX_COLUMNS - 1,
            };
            let row_in = |cells: OverlayCells<'a>| {
                let mut range = cells.range(first..=last);
                if self.reverse {
                    range.next_back()
                } else {
                    range.next()
                }
            };
            let base = row_in(OverlayCells::Base(self.baseline));
            let input = self
                .overlay
                .and_then(|cells| row_in(OverlayCells::Input(cells)));
            let row = match (base.map(|(a, _)| a.row), input.map(|(a, _)| a.row)) {
                (Some(a), Some(b)) => {
                    if self.reverse {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => {
                    self.next_row = None;
                    return None;
                }
            };
            self.next_row = if self.reverse {
                (row > self.area.first.row).then(|| row - 1)
            } else {
                (row < self.area.last.row).then_some(row + 1)
            };
            self.counters.column_seeks += 1;
            let first = CellAddress {
                row,
                col: self.area.first.col,
            };
            let last = CellAddress {
                row,
                col: self.area.last.col,
            };
            let prepare_row =
                |cells: OverlayCells<'a>, selected: Option<(&'a CellAddress, StoredSource<'a>)>| {
                    // A one-column query can use the cell that located its row.
                    // Reopening a singleton range would seek the same B-tree twice.
                    if first == last && selected.is_some_and(|(address, _)| *address == first) {
                        (None, selected)
                    } else {
                        let mut range = Some(cells.range(first..=last));
                        let selected = advance(&mut range, self.reverse);
                        (range, selected)
                    }
                };
            (self.base_row, self.base_next) = prepare_row(OverlayCells::Base(self.baseline), base);
            (self.input_row, self.input_next) = self.overlay.map_or((None, None), |cells| {
                prepare_row(OverlayCells::Input(cells), input)
            });
        }
    }
}

#[cfg(test)]
mod page_column_tests {
    use super::*;
    #[test]
    fn repeated_content_and_style_writes_reuse_payload_allocations() {
        let at = CellAddress { row: 3, col: 4 };
        let mut edits = InputCells::default();
        assert!(!edits.set_cell(at, Cell::new(1.), false).unwrap());
        let allocations = allocation_counter::measure(|| {
            for value in 2..=100 {
                assert!(
                    edits
                        .set_cell(at, Cell::new(f64::from(value)), value % 2 == 0)
                        .unwrap()
                );
            }
        });
        assert_eq!(allocations.count_total, 0);
        assert_eq!(
            edits.get(&at).unwrap().cell().unwrap().value,
            CellValue::Number(100.)
        );
        assert!(edits.get(&at).unwrap().is_style_only());
        assert_eq!(edits.columns.len(), 1);
        let invalid = CellAddress {
            row: crate::model::MAX_ROWS,
            col: 0,
        };
        assert!(edits.set_cell(invalid, Cell::new(0.), false).is_err());
        assert_eq!(edits.cells().len(), 1);
        assert_eq!(edits.columns.len(), 1);
    }

    #[test]
    fn edit_index_tracks_coordinate_membership_across_all_state_transitions() {
        let mut edits = InputCells::default();
        let at = CellAddress { row: 100, col: 3 };
        for edit in [
            CellEdit::Replace(Box::new(Cell::new(1.))),
            CellEdit::StyleOnly(Box::new(Cell::new(1.))),
            CellEdit::Delete,
        ] {
            edits.set(at, edit).unwrap();
            assert_eq!(edits.columns.len(), 1);
            let area = RangeMembership::new(0, at, at).unwrap();
            assert_eq!(InputColumn::new(&edits, area).count(), 1);
        }
        assert!(edits.remove(&at).is_some());
        assert!(edits.remove(&at).is_none());
        assert!(edits.columns.is_empty());
        assert!(
            edits
                .range(at..=CellAddress { row: 0, col: 0 })
                .next()
                .is_none()
        );
        let invalid = CellAddress {
            row: crate::model::MAX_ROWS,
            col: 0,
        };
        assert!(edits.set(invalid, CellEdit::Delete).is_err());
        assert!(edits.cells().is_empty());
        assert!(edits.columns.is_empty());
    }

    #[test]
    fn column_counters_include_source_prefetch_overrides_and_deleted_visits() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for row in [0, 32] {
            sheet.set_value(CellAddress { row, col: 0 }, 1.).unwrap();
        }
        // Unselected columns occupy unrelated pages, which the column index skips.
        sheet
            .set_value(CellAddress { row: 50, col: 8 }, 99.)
            .unwrap();
        let mut input = RunInputs::new(
            &book,
            [
                (0, CellAddress { row: 0, col: 0 }, CellValue::Number(2.)),
                (0, CellAddress { row: 16, col: 0 }, CellValue::Number(3.)),
                (0, CellAddress { row: 40, col: 2 }, CellValue::Number(99.)),
            ],
            3,
            std::sync::Arc::new(()),
        )
        .unwrap();
        input
            .sheets
            .entry(0)
            .or_default()
            .set(CellAddress { row: 32, col: 0 }, CellEdit::Delete)
            .unwrap();
        let area = RangeMembership::new(
            0,
            CellAddress { row: 0, col: 0 },
            CellAddress { row: 63, col: 0 },
        )
        .unwrap();
        let mut cells = EffectiveWorkbook::new(&book, Some(&input))
            .stored(area)
            .unwrap();
        assert_eq!(cells.next().unwrap().1.value, CellValue::Number(2.));
        // Merging A1 consumes both sources and prefetches A33/A17: two
        // baseline pages, one overlay interval and four physical visits.
        let work = QueryCounters {
            row_seeks: 3,
            column_seeks: 2,
            cells_visited: 4,
            key_lookups: 2,
        };
        assert_eq!(cells.counters(), work);
        assert_eq!(cells.next().unwrap().1.value, CellValue::Number(3.));
        assert!(cells.next().is_none());
        assert!(cells.next().is_none());
        // The tombstone is visited but never returned; the column index skips C41.
        assert_eq!(
            cells.counters(),
            QueryCounters {
                cells_visited: 5,
                key_lookups: 3,
                ..work
            }
        );
    }
    #[test]
    fn column_tiles_preserve_overlay_precedence_and_sparse_query_counters() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for row in [0, 24, 25, 26, 49, 50, 74, 75] {
            sheet
                .set_value(CellAddress { row, col: 0 }, row as f64)
                .unwrap();
            sheet
                .set_value(CellAddress { row, col: 3 }, CellValue::Blank)
                .unwrap();
        }
        let assignments = vec![
            (0, CellAddress { row: 25, col: 0 }, CellValue::Blank),
            (0, CellAddress { row: 27, col: 0 }, CellValue::Number(99.)),
            (0, CellAddress { row: 53, col: 2 }, CellValue::Number(7.)),
        ];
        let input = RunInputs::new(&book, assignments.clone(), 3, std::sync::Arc::new(())).unwrap();
        let mut staged = book.clone();
        for (sheet, a, v) in assignments {
            staged.sheet_mut(sheet).unwrap().set_value(a, v).unwrap();
        }
        for col in 0..5 {
            for (first, last) in [(0, 80), (24, 50), (25, 25), (28, 48), (52, 54)] {
                let area = RangeMembership::new(
                    0,
                    CellAddress { row: first, col },
                    CellAddress { row: last, col },
                )
                .unwrap();
                let mut actual = EffectiveWorkbook::new(&book, Some(&input))
                    .stored(area)
                    .unwrap();
                let mut expected = crate::model::query::WorkbookView::new(&staged)
                    .stored(area)
                    .unwrap();
                loop {
                    let a = actual.next();
                    let b = expected.next();
                    assert_eq!(a, b);
                    // Value order agrees although column pages and worksheet row
                    // queries perform different index operations.
                    assert!(
                        actual.counters().key_lookups
                            <= input.sheets.get(&0).unwrap().cells().len() as u64
                    );
                    if a.is_none() {
                        break;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod full_width_tests {
    use super::*;

    fn address(row: u32, col: u32) -> CellAddress {
        CellAddress { row, col }
    }

    #[test]
    fn full_width_intervals_preserve_bounded_effective_sources_in_both_directions() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        let last_col = crate::model::MAX_COLUMNS - 1;
        for (row, col, value) in [
            (0, 0, 1.),
            (1, 0, 2.),
            (1, last_col, 3.),
            (3, 1, 4.),
            (5, last_col, 5.),
            (7, 0, 6.),
        ] {
            sheet.set_value(address(row, col), value).unwrap();
        }
        // These expectations describe authored sources, not saved spill values.
        sheet.set_formula(address(2, 0), "{7,8,9}").unwrap();
        sheet.set_value(address(2, 1), CellValue::Blank).unwrap();
        sheet
            .set_spill_cached_value(address(2, 1), address(2, 0), "{7,8,9}", 8.0.into())
            .unwrap();
        sheet
            .set_spill_cached_value(address(2, 2), address(2, 0), "{7,8,9}", 9.0.into())
            .unwrap();
        let mut input = RunInputs::new(
            &book,
            [
                (0, address(1, 0), 20.0.into()),
                (0, address(2, 2), 90.0.into()),
                (0, address(2, 1), CellValue::Blank),
                (0, address(4, 2), 40.0.into()),
            ],
            4,
            std::sync::Arc::new(()),
        )
        .unwrap();
        input
            .sheets
            .entry(0)
            .or_default()
            .set(address(3, 1), CellEdit::Delete)
            .unwrap();
        let cell = input.cell(0, address(2, 1)).unwrap().clone();
        input
            .sheets
            .entry(0)
            .or_default()
            .set(address(2, 1), CellEdit::StyleOnly(Box::new(cell)))
            .unwrap();
        let expected = [
            (address(0, 0), CellValue::Number(1.), None),
            (address(1, 0), CellValue::Number(20.), None),
            (address(1, last_col), CellValue::Number(3.), None),
            (address(2, 0), CellValue::Blank, Some("{7,8,9}")),
            (address(2, 1), CellValue::Blank, None),
            (address(2, 2), CellValue::Number(90.), None),
            (address(4, 2), CellValue::Number(40.), None),
            (address(5, last_col), CellValue::Number(5.), None),
            (address(7, 0), CellValue::Number(6.), None),
        ];
        let view = EffectiveWorkbook::new(&book, Some(&input));
        for (first, last) in [(0, 7), (1, 5), (2, 2), (3, 3), (6, 6)] {
            for reverse in [false, true] {
                let area =
                    RangeMembership::new(0, address(first, 0), address(last, last_col)).unwrap();
                let mut cells = if reverse {
                    view.stored_reverse(area)
                } else {
                    view.stored(area)
                }
                .unwrap();
                let actual: Vec<_> = cells
                    .by_ref()
                    .map(|(at, cell)| (*at, cell.value.clone(), cell.formula.as_deref()))
                    .collect();
                let mut wanted: Vec<_> = expected
                    .iter()
                    .filter(|(at, _, _)| at.row >= first && at.row <= last)
                    .cloned()
                    .collect();
                if reverse {
                    wanted.reverse();
                }
                assert_eq!(actual, wanted);
                assert_eq!(
                    cells.counters(),
                    QueryCounters {
                        row_seeks: 2,
                        cells_visited: wanted.len() as u64,
                        ..QueryCounters::default()
                    }
                );
                assert!(cells.next().is_none());
            }
        }
    }

    #[test]
    fn contiguous_empty_queries_and_narrow_neighbors_keep_their_work_contracts() {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        let full =
            RangeMembership::new(0, address(1, 0), address(3, crate::model::MAX_COLUMNS - 1))
                .unwrap();
        for reverse in [false, true] {
            let view = EffectiveWorkbook::new(&book, None);
            let mut cells = if reverse {
                view.stored_reverse(full)
            } else {
                view.stored(full)
            }
            .unwrap();
            assert!(cells.next().is_none());
            assert!(cells.next().is_none());
            assert_eq!(
                cells.counters(),
                QueryCounters {
                    row_seeks: 1,
                    ..QueryCounters::default()
                }
            );
        }
        let missing = RangeMembership { sheet: 1, ..full };
        assert!(EffectiveWorkbook::new(&book, None).stored(missing).is_err());
        assert!(
            EffectiveWorkbook::new(&book, None)
                .stored_reverse(missing)
                .is_err()
        );
        let sheet = book.sheet_mut(0).unwrap();
        for row in [1, 2] {
            for col in [0, 1, 2] {
                sheet
                    .set_value(address(row, col), f64::from(row * 10 + col))
                    .unwrap();
            }
        }
        let narrow = RangeMembership::new(0, address(1, 1), address(3, 2)).unwrap();
        let mut cells = EffectiveWorkbook::new(&book, None).stored(narrow).unwrap();
        assert_eq!(
            cells.by_ref().map(|(at, _)| *at).collect::<Vec<_>>(),
            vec![address(1, 1), address(1, 2), address(2, 1), address(2, 2)]
        );
        assert_eq!(
            cells.counters(),
            QueryCounters {
                row_seeks: 3,
                column_seeks: 2,
                cells_visited: 4,
                key_lookups: 0
            }
        );
    }
}
