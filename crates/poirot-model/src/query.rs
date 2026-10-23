//! Borrowed, uncached worksheet queries. Plans describe membership, including absent cells.
use crate::CellRange;
use crate::{Cell, CellAddress, CellValue, ModelError, Workbook, Worksheet};

/// An inclusive rectangular region on one zero-based sheet index.
/// Membership includes absent cells; stored-cell iteration visits only present cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RangeMembership {
    /// Zero-based sheet index containing this rectangle.
    pub sheet: usize,
    /// Top-left inclusive coordinate.
    pub first: CellAddress,
    /// Bottom-right inclusive coordinate.
    pub last: CellAddress,
}
impl RangeMembership {
    /// Validate an ordered inclusive rectangle within worksheet bounds.
    pub fn new(sheet: usize, first: CellAddress, last: CellAddress) -> Result<Self, ModelError> {
        first.validate()?;
        last.validate()?;
        if first.row > last.row || first.col > last.col {
            return Err(ModelError("query range endpoints must be ordered".into()));
        }
        Ok(Self { sheet, first, last })
    }
    /// Test coordinate membership without consulting stored cell occupancy.
    pub fn contains(self, sheet: usize, address: CellAddress) -> bool {
        self.sheet == sheet
            && (self.first.row..=self.last.row).contains(&address.row)
            && (self.first.col..=self.last.col).contains(&address.col)
    }
    /// Count logical coordinates, including absent cells.
    pub fn size(self) -> u64 {
        (u64::from(self.last.row) - u64::from(self.first.row) + 1)
            * (u64::from(self.last.col) - u64::from(self.first.col) + 1)
    }
}

/// Areas remain in source order; overlapping membership deliberately repeats cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    areas: Vec<RangeMembership>,
}
impl QueryPlan {
    /// Capture ordered range memberships without expanding their cells.
    pub fn new(areas: Vec<RangeMembership>) -> Result<Self, ModelError> {
        for area in &areas {
            RangeMembership::new(area.sheet, area.first, area.last)?;
        }
        Ok(Self { areas })
    }
    /// Borrow the query's ordered rectangles, retaining overlaps and duplicates.
    pub fn memberships(&self) -> &[RangeMembership] {
        &self.areas
    }
    /// Validate before any dense allocation. This limit is operational, not a cell error.
    pub fn dense_size(&self, budget: DenseBudget) -> Result<usize, ModelError> {
        let size = self
            .areas
            .iter()
            .try_fold(0u64, |total, area| total.checked_add(area.size()))
            .ok_or_else(|| ModelError("dense query size overflow".into()))?;
        if size > budget.max_cells {
            return Err(ModelError("dense query cell budget exceeded".into()));
        }
        usize::try_from(size).map_err(|_| ModelError("dense query exceeds address space".into()))
    }
}
/// Maximum logical cell count allowed when expanding a sparse query to dense storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DenseBudget {
    /// Maximum logical coordinates admitted for dense expansion.
    pub max_cells: u64,
}
/// Deterministic costs of sparse traversal: tree seeks, stored visits, and key lookups.
/// These counters describe query work, not elapsed time or CPU instructions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct QueryCounters {
    /// Sparse row-index or row-page seeks performed by this query.
    pub row_seeks: u64,
    /// Sparse column-index seeks performed by this query.
    pub column_seeks: u64,
    /// Stored cells visited during traversal.
    pub cells_visited: u64,
    /// Direct coordinate lookups performed by this query.
    pub key_lookups: u64,
}
/// Counts absent coordinates, explicit blank literals, and formula cells separately.
/// A formula with a saved blank is still a formula, not an ordinary blank literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlankCounts {
    /// Logical coordinates with no stored cell.
    pub absent: u64,
    /// Explicitly stored cells whose value is blank.
    pub stored_blank: u64,
    /// Formula caches require evaluator interpretation and are not counted as stored blanks.
    pub formula_cells: u64,
}
/// Read port used by query consumers; implementations return borrowed original cells.
pub trait WorkbookRead {
    /// Borrow a worksheet by index through the query's source abstraction.
    fn worksheet(&self, sheet: usize) -> Option<&Worksheet>;
}
impl WorkbookRead for Workbook {
    fn worksheet(&self, sheet: usize) -> Option<&Worksheet> {
        self.sheet(sheet)
    }
}
/// Borrows a workbook read port for sparse queries without evaluating formulas.
/// Unlike the evaluator's prepared view, this view needs no compiled workbook plan.
pub struct WorkbookView<'a, R: WorkbookRead + ?Sized> {
    source: &'a R,
}
impl<'a, R: WorkbookRead + ?Sized> WorkbookView<'a, R> {
    /// Borrow a workbook-readable source without copying its cells.
    pub fn new(source: &'a R) -> Self {
        Self { source }
    }
    /// Traverse stored cells for the plan without densely expanding blanks.
    pub fn stored(&self, area: RangeMembership) -> Result<StoredCells<'a>, ModelError> {
        RangeMembership::new(area.sheet, area.first, area.last)?;
        let sheet = self
            .source
            .worksheet(area.sheet)
            .ok_or_else(|| ModelError("query worksheet index out of bounds".into()))?;
        Ok(StoredCells {
            sheet,
            area,
            next_row: Some(area.first.row),
            row: None,
            counters: QueryCounters::default(),
        })
    }
    /// Borrowed reverse membership traversal, without a retained index or value-cache reads.
    pub fn stored_reverse(
        &self,
        area: RangeMembership,
    ) -> Result<StoredCellsReverse<'a>, ModelError> {
        RangeMembership::new(area.sheet, area.first, area.last)?;
        let sheet = self
            .source
            .worksheet(area.sheet)
            .ok_or_else(|| ModelError("query worksheet index out of bounds".into()))?;
        Ok(StoredCellsReverse {
            sheet,
            area,
            next_row: Some(area.last.row),
            row: None,
            counters: QueryCounters::default(),
        })
    }
    /// Look up one coordinate while retaining absence as None.
    pub fn lookup(
        &self,
        sheet: usize,
        address: CellAddress,
        counters: &mut QueryCounters,
    ) -> Result<Option<&'a Cell>, ModelError> {
        address.validate()?;
        let sheet = self
            .source
            .worksheet(sheet)
            .ok_or_else(|| ModelError("query worksheet index out of bounds".into()))?;
        counters.key_lookups += 1;
        Ok(sheet.get(address))
    }
    /// Count absent and explicit blank members under the supplied dense budget.
    pub fn blank_counts(
        &self,
        area: RangeMembership,
    ) -> Result<(BlankCounts, QueryCounters), ModelError> {
        let mut cells = self.stored(area)?;
        let mut counts = BlankCounts {
            absent: area.size(),
            stored_blank: 0,
            formula_cells: 0,
        };
        for (_, cell) in cells.by_ref() {
            counts.absent -= 1;
            if cell.formula.is_some() {
                counts.formula_cells += 1;
            } else if cell.value == CellValue::Blank {
                counts.stored_blank += 1;
            }
        }
        Ok((counts, cells.counters()))
    }
}
/// Each occupied row is located by a seek; only the requested columns are traversed.
/// No retained index or revision counter can become stale after model mutations.
pub struct StoredCells<'a> {
    sheet: &'a Worksheet,
    area: RangeMembership,
    next_row: Option<u32>,
    row: Option<CellRange<'a>>,
    counters: QueryCounters,
}
impl StoredCells<'_> {
    /// Inspect sparse traversal work accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }
}
impl<'a> Iterator for StoredCells<'a> {
    /// Borrowed stored coordinate and cell, retaining explicit blanks.
    type Item = (&'a CellAddress, &'a Cell);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(row) = &mut self.row
                && let Some(cell) = row.next()
            {
                self.counters.cells_visited += 1;
                return Some(cell);
            }
            let next = self.next_row?;
            self.counters.row_seeks += 1;
            let end = CellAddress {
                row: self.area.last.row,
                col: crate::MAX_COLUMNS - 1,
            };
            let Some((address, _)) = self
                .sheet
                .cells()
                .range(CellAddress { row: next, col: 0 }..=end)
                .next()
            else {
                self.next_row = None;
                return None;
            };
            let row = address.row;
            self.next_row = (row < self.area.last.row).then_some(row + 1);
            self.counters.column_seeks += 1;
            self.row = Some(self.sheet.cells().range(
                CellAddress {
                    row,
                    col: self.area.first.col,
                }..=CellAddress {
                    row,
                    col: self.area.last.col,
                },
            ));
        }
    }
}

/// Reverse counterpart to StoredCells: seeks occupied rows and visits only selected columns.
pub struct StoredCellsReverse<'a> {
    sheet: &'a Worksheet,
    area: RangeMembership,
    next_row: Option<u32>,
    row: Option<CellRange<'a>>,
    counters: QueryCounters,
}
impl StoredCellsReverse<'_> {
    /// Inspect reverse sparse traversal work accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }
}
impl<'a> Iterator for StoredCellsReverse<'a> {
    /// Borrowed stored coordinate and cell in reverse query order.
    type Item = (&'a CellAddress, &'a Cell);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(row) = &mut self.row
                && let Some(cell) = row.next_back()
            {
                self.counters.cells_visited += 1;
                return Some(cell);
            }
            let next = self.next_row?;
            self.counters.row_seeks += 1;
            let first = CellAddress {
                row: self.area.first.row,
                col: 0,
            };
            let Some((address, _)) = self
                .sheet
                .cells()
                .range(
                    first..=CellAddress {
                        row: next,
                        col: crate::MAX_COLUMNS - 1,
                    },
                )
                .next_back()
            else {
                self.next_row = None;
                return None;
            };
            let row = address.row;
            self.next_row = (row > self.area.first.row).then(|| row - 1);
            self.counters.column_seeks += 1;
            self.row = Some(self.sheet.cells().range(
                CellAddress {
                    row,
                    col: self.area.first.col,
                }..=CellAddress {
                    row,
                    col: self.area.last.col,
                },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reverse_membership_is_borrowed_ordered_and_excludes_other_columns() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Data").unwrap();
        for row in [0, 2, 9] {
            sheet.set_value(address(row, 3), 1.).unwrap();
        }
        sheet.set_formula(address(5, 3), "RAND()").unwrap();
        sheet.set_value(address(8, 4), 2.).unwrap();
        let bounds = area(address(0, 3), address(9, 3));
        let selected: Vec<_> = WorkbookView::new(&book)
            .stored_reverse(bounds)
            .unwrap()
            .map(|(a, _)| *a)
            .collect();
        assert_eq!(
            selected,
            vec![address(9, 3), address(5, 3), address(2, 3), address(0, 3)]
        );
        book.sheet_mut(0).unwrap().remove(address(5, 3));
        let selected: Vec<_> = WorkbookView::new(&book)
            .stored_reverse(bounds)
            .unwrap()
            .map(|(a, _)| *a)
            .collect();
        assert_eq!(selected, vec![address(9, 3), address(2, 3), address(0, 3)]);
    }

    use super::*;
    fn address(row: u32, col: u32) -> CellAddress {
        CellAddress { row, col }
    }
    fn area(first: CellAddress, last: CellAddress) -> RangeMembership {
        RangeMembership::new(0, first, last).unwrap()
    }
    #[test]
    fn narrow_query_seeks_past_unrelated_rows_and_columns() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Data").unwrap();
        for row in 0..1000 {
            sheet.set_value(address(row, 500), 9.0).unwrap();
        }
        for col in 0..1000 {
            sheet.set_value(address(500, col), 2.0).unwrap();
        }
        let view = WorkbookView::new(&book);
        let mut cells = view.stored(area(address(500, 2), address(500, 3))).unwrap();
        let selected: Vec<_> = cells.by_ref().map(|(a, c)| (*a, &c.value)).collect();
        assert_eq!(
            selected,
            vec![
                (address(500, 2), &CellValue::Number(2.0)),
                (address(500, 3), &CellValue::Number(2.0))
            ]
        );
        assert_eq!(cells.counters().cells_visited, 2);
        assert_eq!(cells.counters().row_seeks, 1);
        assert_eq!(cells.counters().column_seeks, 1);
        assert!(book.sheet(0).unwrap().cells().len() > 1900);
    }
    #[test]
    fn whole_column_skips_empty_rows_and_describes_blank_and_formula_cells() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Data").unwrap();
        sheet.set(address(3, 0), Cell::default()).unwrap();
        sheet
            .set_formula(address(crate::MAX_ROWS - 1, 0), "1")
            .unwrap();
        sheet.set_value(address(3, 900), "unrelated").unwrap();
        let view = WorkbookView::new(&book);
        let (counts, counters) = view
            .blank_counts(area(address(0, 0), address(crate::MAX_ROWS - 1, 0)))
            .unwrap();
        assert_eq!(
            counts,
            BlankCounts {
                absent: u64::from(crate::MAX_ROWS) - 2,
                stored_blank: 1,
                formula_cells: 1
            }
        );
        assert_eq!(counters.cells_visited, 2);
        assert_eq!(counters.row_seeks, 2);
    }
    #[test]
    fn absent_membership_and_fresh_reads_survive_insert_mutate_and_remove() {
        let mut book = Workbook::new();
        book.add_sheet("Data").unwrap();
        let bounds = area(address(0, 0), address(9, 0));
        let absent = address(4, 0);
        assert!(bounds.contains(0, absent));
        assert!(!bounds.contains(1, absent));
        assert_eq!(
            WorkbookView::new(&book)
                .blank_counts(bounds)
                .unwrap()
                .0
                .absent,
            10
        );
        book.sheet_mut(0).unwrap().set_value(absent, 3.0).unwrap();
        book.sheet_mut(0).unwrap().get_mut(absent).unwrap().value = CellValue::Blank;
        let mut counters = QueryCounters::default();
        assert_eq!(
            WorkbookView::new(&book)
                .lookup(0, absent, &mut counters)
                .unwrap()
                .unwrap()
                .value,
            CellValue::Blank
        );
        assert_eq!(counters.key_lookups, 1);
        assert_eq!(
            WorkbookView::new(&book)
                .blank_counts(bounds)
                .unwrap()
                .0
                .stored_blank,
            1
        );
        book.sheet_mut(0).unwrap().remove(absent);
        assert_eq!(
            WorkbookView::new(&book)
                .blank_counts(bounds)
                .unwrap()
                .0
                .absent,
            10
        );
    }
    #[test]
    fn overlapping_plan_preserves_order_duplicates_and_checks_dense_budget() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("Data").unwrap();
        for row in 0..3 {
            sheet.set_value(address(row, 0), f64::from(row)).unwrap();
        }
        let plan = QueryPlan::new(vec![
            area(address(1, 0), address(2, 0)),
            area(address(0, 0), address(1, 0)),
        ])
        .unwrap();
        let view = WorkbookView::new(&book);
        let selected: Vec<_> = plan
            .memberships()
            .iter()
            .flat_map(|a| view.stored(*a).unwrap().map(|(a, _)| *a))
            .collect();
        assert_eq!(
            selected,
            vec![address(1, 0), address(2, 0), address(0, 0), address(1, 0)]
        );
        assert_eq!(plan.dense_size(DenseBudget { max_cells: 4 }).unwrap(), 4);
        assert!(plan.dense_size(DenseBudget { max_cells: 3 }).is_err());
        let full = QueryPlan::new(vec![area(
            address(0, 0),
            address(crate::MAX_ROWS - 1, crate::MAX_COLUMNS - 1),
        )])
        .unwrap();
        assert!(full.dense_size(DenseBudget { max_cells: 1000 }).is_err());
    }
    #[test]
    fn invalid_ranges_fail_before_traversal() {
        assert!(RangeMembership::new(0, address(2, 1), address(1, 1)).is_err());
        assert!(RangeMembership::new(0, address(0, 0), address(crate::MAX_ROWS, 0)).is_err());
        let book = Workbook::new();
        assert!(
            WorkbookView::new(&book)
                .stored(area(address(0, 0), address(0, 0)))
                .is_err()
        );
    }
}
