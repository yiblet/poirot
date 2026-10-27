//! Full-width interval traversal in both directions.

use super::super::cell_edit::CellEdit;
use super::super::effective_workbook::EffectiveWorkbook;
use super::super::run_inputs::RunInputs;
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{CellAddress, CellValue, Workbook};

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
            let area = RangeMembership::new(0, address(first, 0), address(last, last_col)).unwrap();
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
        RangeMembership::new(0, address(1, 0), address(3, crate::model::MAX_COLUMNS - 1)).unwrap();
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
