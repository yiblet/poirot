//! Invalidation follows inclusive rectangle membership on both axes and sheets.
//! Expected numeric sums are elementary addition under SUM's documented contract:
//! https://support.microsoft.com/en-us/excel/functions/sum-function
use poirot_eval::{
    EvaluationTarget,
    overlay::{EvaluationContext, Overlay},
};
use poirot_model::{CellValue, Workbook};

fn at(sheet: usize, address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet,
        address: address.parse().unwrap(),
    }
}

#[test]
fn rectangular_dependencies_survive_insertions_transitive_reads_and_snapshot_restore() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [("A1", 1.), ("B1", 2.), ("A2", 3.), ("B2", 4.), ("F1", 7.)] {
        sheet.set_value(at(0, cell).address, value).unwrap();
    }
    for (cell, formula) in [
        ("H1", "SUM(A1:A4)"),
        ("H2", "SUM(A1:D1)"),
        ("H3", "SUM(A1:B2)"),
        ("H4", "SUM(C3:D4)"),
        ("H5", "H3*2"),
        ("H6", "SUM(F1:F4)"),
    ] {
        sheet.set_formula(at(0, cell).address, formula).unwrap();
    }
    book.add_sheet("Other")
        .unwrap()
        .set_formula(at(1, "B1").address, "SUM(S!A1:B2)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let targets = [
        at(0, "H1"),
        at(0, "H2"),
        at(0, "H3"),
        at(0, "H4"),
        at(0, "H5"),
        at(0, "H6"),
        at(1, "B1"),
    ];
    for (edit, expected) in [
        (None, [4., 3., 10., 0., 20., 7., 10.]),
        (Some(("A3", 5.)), [9., 3., 10., 0., 20., 7., 10.]),
        (Some(("C1", 6.)), [9., 9., 10., 0., 20., 7., 10.]),
        (Some(("D4", 7.)), [9., 9., 10., 7., 20., 7., 10.]),
        (Some(("B2", 9.)), [9., 9., 15., 7., 30., 7., 15.]),
    ] {
        if let Some((cell, value)) = edit {
            overlay.set_value(at(0, cell), value).unwrap();
        }
        let values = overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&targets)
            .unwrap();
        assert_eq!(
            values.into_iter().map(|x| x.value).collect::<Vec<_>>(),
            expected.map(CellValue::Number)
        );
    }
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut overlay = state.attach(book).unwrap();
    overlay.set_value(at(0, "A1"), 11.).unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "H5")).unwrap(), CellValue::Number(50.));
    assert_eq!(run.cell(at(1, "B1")).unwrap(), CellValue::Number(25.));
    assert_eq!(run.cell(at(0, "H6")).unwrap(), CellValue::Number(7.));
    drop(run);
    // A changed column outside every source rectangle must leave unrelated results retained.
    overlay.set_value(at(0, "Z1"), 99.).unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "H6")).unwrap(), CellValue::Number(7.));
    assert_eq!(run.usage().formula_executions, 0);
}

#[test]
fn disjoint_columns_on_either_side_preserve_range_consumers() {
    for (input, formula, inserted) in [("A1", "SUM(A1:A3)", "A3"), ("C1", "SUM(C1:C3)", "C3")] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(at(0, input).address, 2.).unwrap();
        sheet.set_formula(at(0, "B1").address, formula).unwrap();
        sheet.set_formula(at(0, "B2").address, "B1*3").unwrap();
        let mut overlay = Overlay::with_workbook(book).build().unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(0, "B2"))
                .unwrap(),
            CellValue::Number(6.)
        );
        overlay.set_value(at(0, inserted), 5.).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(0, "B2"))
                .unwrap(),
            CellValue::Number(21.)
        );
        overlay.set_value(at(0, input), 4.).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(0, "B2"))
                .unwrap(),
            CellValue::Number(27.)
        );
    }
}

#[test]
fn spill_owner_outside_the_consumed_rectangle_still_invalidates_its_readers() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at(0, "A1").address, "{1,2;3,4}").unwrap();
    sheet
        .set_formula(at(0, "Z1").address, "SUM(B2:B3)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    for (formula, expected) in [("{1,2;3,4}", 4.), ("{2,4;6,8}", 8.), ("9", 0.)] {
        overlay.set_formula(at(0, "A1"), formula).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at(0, "Z1"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
    overlay.set_value(at(0, "B3"), 3.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "Z1"))
            .unwrap(),
        CellValue::Number(3.)
    );
}

#[test]
fn multiple_row_zero_ranges_remain_correct_after_partial_edits_and_restore() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("G1", "SUM(A1:A2)+SUM(C1:C2)+A1"),
        ("G2", "SUM(C1:C2)"),
        ("H1", "SUM(G1:G2)"),
    ] {
        sheet.set_formula(at(0, cell).address, formula).unwrap();
    }
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let targets = [at(0, "G1"), at(0, "G2"), at(0, "H1")];
    // Blank-to-value, formula insertion/replacement/removal and two distinct
    // source rectangles. A1 contributes twice; C1/C2 contribute once to each G.
    for (cell, formula, value, expected) in [
        ("B1", None, 99., [0., 0., 0.]),
        ("A1", None, 2., [4., 0., 4.]),
        ("C1", Some("3"), 0., [7., 3., 10.]),
        ("C1", Some("5"), 0., [9., 5., 14.]),
        ("C2", None, 7., [16., 12., 28.]),
        ("A1", None, 4., [20., 12., 32.]),
    ] {
        if let Some(formula) = formula {
            overlay.set_formula(at(0, cell), formula).unwrap();
        } else {
            overlay.set_value(at(0, cell), value).unwrap();
        }
        let mut run = overlay.evaluation(&context).unwrap();
        for (target, value) in targets.into_iter().zip(expected) {
            assert_eq!(run.cell(target).unwrap(), CellValue::Number(value));
        }
    }
    overlay.remove_cell(at(0, "C1")).unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at(0, "H1")).unwrap(), CellValue::Number(22.));
    drop(run);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(book).unwrap();
    restored.set_value(at(0, "C1"), 6.).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "H1"))
            .unwrap(),
        CellValue::Number(34.)
    );
    restored.set_value(at(0, "A1"), 1.).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at(0, "H1"))
            .unwrap(),
        CellValue::Number(28.)
    );
}
