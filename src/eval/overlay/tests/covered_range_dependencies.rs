//! Range membership replaces redundant internal points without losing edit propagation.
//! Sums below use integer addition under the documented SUM reference contract:
//! https://support.microsoft.com/en-us/excel/functions/sum-function
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::incremental::Overlay;
use crate::model::{CellValue, Workbook};

fn at(sheet: usize, address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet,
        address: address.parse().unwrap(),
    }
}

#[test]
fn overlapping_ranges_keep_outside_points_and_nested_cross_sheet_dependencies() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [("A1", 1.), ("A3", 3.), ("C1", 5.), ("G1", 7.)] {
        sheet.set_value(at(0, address).address, value).unwrap();
    }
    // C1 is read both before and after membership. A2 is a nested formula
    // whose G1 dependency lies outside all the parent's range guards.
    sheet.set_formula(at(0, "A2").address, "G1*2").unwrap();
    sheet
        .set_formula(at(0, "Z1").address, "C1+SUM(A1:A3,A3:A5,C1:C2)+C1")
        .unwrap();
    book.add_sheet("Other")
        .unwrap()
        .set_formula(at(1, "A1").address, "S!Z1*2")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    for (edit, expected) in [
        (None, 72.),
        (Some(("A4", 4.)), 80.),
        (Some(("G1", 10.)), 92.),
        (Some(("C1", 6.)), 98.),
        (Some(("A3", 5.)), 106.),
        (Some(("C2", 8.)), 122.),
    ] {
        if let Some((address, value)) = edit {
            overlay.set_value(at(0, address), value).unwrap();
        }
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(1, "A1"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
    let snapshot = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&snapshot).unwrap()).unwrap();
    let mut restored = state.attach(book).unwrap();
    restored.set_value(at(0, "G1"), 11.).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at(1, "A1"))
            .unwrap(),
        CellValue::Number(126.)
    );
    // An unrelated edit does not evict the compacted traces.
    restored.set_value(at(0, "Y50"), 99.).unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(1, "A1")).unwrap(), CellValue::Number(126.));
    assert_eq!(run.usage().formula_executions, 0);
}
