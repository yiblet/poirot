//! Paged column traversal and edit payload reuse.

use super::super::cell_edit::CellEdit;
use super::super::effective_workbook::EffectiveWorkbook;
use super::super::input_cells::InputCells;
use super::super::input_column::InputColumn;
use super::super::run_inputs::RunInputs;
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{Cell, CellAddress, CellValue, Workbook};

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
