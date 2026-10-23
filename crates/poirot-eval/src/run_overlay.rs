//! Immutable run assignments and borrowed effective cell/membership reads.
use poirot_model::CellStore;
use poirot_model::query::{QueryCounters, RangeMembership};
use poirot_model::{Cell, CellAddress, CellValue, ModelError, Workbook};
use std::collections::{BTreeMap, BTreeSet};
/// One sheet's sparse authored overrides using smaller overlay pages.
type InputCells = CellStore<Cell, 16, 2>;
/// Borrowed coordinate-ordered view of overlay inputs within a rectangle.
type InputRange<'a> = poirot_model::CellRange<'a, Cell, 16, 2>;

/// Resolved, unique assignments admitted against caller-supplied assignment limits.
pub struct RunInputs {
    /// Sparse authored cell overrides, using overlay-sized pages.
    pub(crate) sheets: BTreeMap<usize, InputCells>,
    /// Baseline cells hidden by deletions.
    pub(crate) deleted: BTreeSet<(usize, CellAddress)>,
    /// Overrides that change only style, preserving baseline content and saved ownership.
    pub(crate) style_only: BTreeSet<(usize, CellAddress)>,
    /// Source identity token binding assignments to their shared workbook owner.
    pub(crate) source: std::sync::Arc<()>,
}
impl RunInputs {
    /// Create empty sparse inputs with a fresh source token.
    pub(crate) fn empty() -> Self {
        Self {
            sheets: BTreeMap::new(),
            deleted: BTreeSet::new(),
            style_only: BTreeSet::new(),
            source: std::sync::Arc::new(()),
        }
    }
    /// Validate unique literal assignments and preserve baseline cell styles.
    pub(crate) fn new(
        workbook: &Workbook,
        assignments: impl IntoIterator<Item = (usize, CellAddress, CellValue)>,
        max_assignments: usize,
        source: std::sync::Arc<()>,
    ) -> Result<Self, ModelError> {
        let mut inputs = Self {
            sheets: BTreeMap::new(),
            deleted: BTreeSet::new(),
            style_only: BTreeSet::new(),
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
                .set(address, cell)?
                .is_some()
            {
                return Err(ModelError("duplicate input assignment".into()));
            }
        }
        Ok(inputs)
    }
    /// Read an explicit override without falling back to the baseline.
    pub fn cell(&self, sheet: usize, address: CellAddress) -> Option<&Cell> {
        self.sheets.get(&sheet)?.get(&address)
    }
}

/// Metadata remains baseline-owned. Projection and evaluation share this read port.
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
        if self
            .inputs
            .is_some_and(|i| i.deleted.contains(&(sheet, address)))
        {
            return None;
        }
        self.inputs
            .and_then(|inputs| inputs.cell(sheet, address))
            .or_else(|| self.workbook.sheet(sheet)?.source_cell(address))
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
        let contiguous = area.first.col == 0 && area.last.col == poirot_model::MAX_COLUMNS - 1;
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
            deleted: self.inputs.map(|i| &i.deleted),
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

/// Borrowed effective cells with counters for the traversal actually performed.
/// Column queries count source page seeks and visits, including merge prefetches;
/// Narrow rectangles seek occupied rows; full-width rectangles open contiguous
/// source intervals. Both visit the effective union after deletion/source filtering.
/// Both ranges borrow their source. No retained membership index or per-read allocation.
pub struct EffectiveStoredCells<'a> {
    baseline: poirot_model::WorksheetCells<'a>,
    baseline_sheet: &'a poirot_model::Worksheet,
    overlay: Option<&'a InputCells>,
    deleted: Option<&'a BTreeSet<(usize, CellAddress)>>,
    area: RangeMembership,
    reverse: bool,
    next_row: Option<u32>,
    base_row: Option<OverlayRange<'a>>,
    input_row: Option<OverlayRange<'a>>,
    base_next: Option<(&'a CellAddress, &'a Cell)>,
    input_next: Option<(&'a CellAddress, &'a Cell)>,
    column: Option<ColumnTraversal<'a>>,
    counters: QueryCounters,
}
impl EffectiveStoredCells<'_> {
    /// Return sparse traversal counters accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }
}
/// Merged base/overlay cell iterator preserving source precedence.
#[derive(Clone, Copy)]
enum OverlayCells<'a> {
    Base(poirot_model::WorksheetCells<'a>),
    Input(&'a InputCells),
}
impl<'a> OverlayCells<'a> {
    fn range(self, range: std::ops::RangeInclusive<CellAddress>) -> OverlayRange<'a> {
        match self {
            Self::Base(c) => OverlayRange::Base(c.range(range)),
            Self::Input(c) => OverlayRange::Input(c.cells().range(range)),
        }
    }
}
/// Merged bounded base/overlay iterator used by sparse range consumers.
enum OverlayRange<'a> {
    Base(poirot_model::CellRange<'a>),
    Input(InputRange<'a>),
}
impl<'a> Iterator for OverlayRange<'a> {
    /// Borrowed effective coordinate and cell after overlay precedence.
    type Item = (&'a CellAddress, &'a Cell);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r.next(),
            Self::Input(r) => r.next(),
        }
    }
}
impl DoubleEndedIterator for OverlayRange<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Base(r) => r.next_back(),
            Self::Input(r) => r.next_back(),
        }
    }
}
fn advance<'a>(
    range: &mut Option<OverlayRange<'a>>,
    reverse: bool,
) -> Option<(&'a CellAddress, &'a Cell)> {
    let range = range.as_mut()?;
    if reverse {
        range.next_back()
    } else {
        range.next()
    }
}
/// Merge baseline and override column cursors without a separate accounting walk.
struct ColumnTraversal<'a> {
    base: poirot_model::ColumnCells<'a>,
    input: Option<poirot_model::ColumnCells<'a, Cell, 16, 2>>,
    base_next: Option<(&'a CellAddress, &'a Cell)>,
    input_next: Option<(&'a CellAddress, &'a Cell)>,
    started: bool,
    finished: bool,
}
impl<'a> ColumnTraversal<'a> {
    fn new(
        base: poirot_model::WorksheetCells<'a>,
        input: Option<&'a InputCells>,
        area: RangeMembership,
    ) -> Self {
        Self {
            base: base.column(area.first.col, area.first.row, area.last.row),
            input: input.map(|c| {
                c.cells()
                    .column(area.first.col, area.first.row, area.last.row)
            }),
            base_next: None,
            input_next: None,
            started: false,
            finished: false,
        }
    }
    fn next(&mut self, counters: &mut QueryCounters) -> Option<(&'a CellAddress, &'a Cell)> {
        if self.finished {
            return None;
        }
        if !self.started {
            self.started = true;
            self.base_next = self.base.next();
            self.input_next = self.input.as_mut().and_then(Iterator::next);
        }
        let selected = match (self.base_next, self.input_next) {
            (Some(base), Some(input)) if base.0 == input.0 => {
                self.base_next = self.base.next();
                self.input_next = self.input.as_mut().and_then(Iterator::next);
                Some(input)
            }
            (Some(base), Some(input)) if base.0 < input.0 => {
                self.base_next = self.base.next();
                Some(base)
            }
            (_, Some(input)) => {
                self.input_next = self.input.as_mut().and_then(Iterator::next);
                Some(input)
            }
            (Some(base), None) => {
                self.base_next = self.base.next();
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
                if !self
                    .deleted
                    .is_some_and(|d| d.contains(&(self.area.sheet, *item.0)))
                {
                    if self.baseline_sheet.spill_owner(*item.0).is_none() {
                        return Some(item);
                    }
                    if self
                        .overlay
                        .is_some_and(|cells| cells.get(item.0).is_some())
                    {
                        return Some(item);
                    }
                    if let Some(source) = self.baseline_sheet.source_cell(*item.0) {
                        return Some((item.0, source));
                    }
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
                if self
                    .deleted
                    .is_some_and(|d| d.contains(&(self.area.sheet, *item.0)))
                {
                    continue;
                }
                let item = if self.baseline_sheet.spill_owner(*item.0).is_none()
                    || self
                        .overlay
                        .is_some_and(|cells| cells.get(item.0).is_some())
                {
                    item
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
                col: poirot_model::MAX_COLUMNS - 1,
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
                |cells: OverlayCells<'a>, selected: Option<(&'a CellAddress, &'a Cell)>| {
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
        input.deleted.insert((0, CellAddress { row: 32, col: 0 }));
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
        // Merging A1 consumes both sources and prefetches A33/A17. That is
        // two column bound seeks, four loaded pages, four physical stored visits.
        let work = QueryCounters {
            row_seeks: 4,
            column_seeks: 2,
            cells_visited: 4,
            key_lookups: 0,
        };
        assert_eq!(cells.counters(), work);
        assert_eq!(cells.next().unwrap().1.value, CellValue::Number(3.));
        assert!(cells.next().is_none());
        assert!(cells.next().is_none());
        // Hiding A33 and replacing A1 cannot undo physical source visits.
        assert_eq!(cells.counters(), work);
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
                let mut expected = poirot_model::query::WorkbookView::new(&staged)
                    .stored(area)
                    .unwrap();
                loop {
                    let a = actual.next();
                    let b = expected.next();
                    assert_eq!(a, b);
                    // Value order agrees although column pages and worksheet row
                    // queries perform different index operations.
                    assert_eq!(actual.counters().key_lookups, 0);
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
        let last_col = poirot_model::MAX_COLUMNS - 1;
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
        input.deleted.insert((0, address(3, 1)));
        input.style_only.insert((0, address(2, 1)));
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
            RangeMembership::new(0, address(1, 0), address(3, poirot_model::MAX_COLUMNS - 1))
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
