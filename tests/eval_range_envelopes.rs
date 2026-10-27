//! Colon envelopes follow pinned ClosedXML ReferenceOperatorsTests and independent
//! Calc probes. Cross-sheet rejection and blank publication retain project policy.
use poirot::eval::overlay::Overlay;
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};

fn book(cell_named_sheet: bool) -> Workbook {
    let mut w = Workbook::new();
    if cell_named_sheet {
        w.add_sheet("C5").unwrap();
    }
    w.add_sheet("S").unwrap();
    let sheet = usize::from(cell_named_sheet);
    w.add_sheet("Other").unwrap();
    for row in 0..5 {
        for col in 0..4 {
            w.sheet_mut(sheet)
                .unwrap()
                .set_value(poirot::model::CellAddress { row, col }, 1.)
                .unwrap();
        }
    }
    if cell_named_sheet {
        w.sheet_mut(0)
            .unwrap()
            .set_value("D3".parse().unwrap(), 100.)
            .unwrap();
    }
    w
}
fn value(w: &Workbook, sheet: usize, source: &str) -> CellValue {
    match run_formula(
        w,
        sheet,
        "Z100".parse().unwrap(),
        source,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{source}: {other:?}"),
    }
}
#[test]
fn range_operator_envelopes_chains_and_union_operands() {
    let w = book(false);
    for (source, expected) in [
        ("SUM(S!B2:C5:S!D3)", 12.),
        ("SUM((S!B2:C5):S!D3)", 12.),
        ("SUM(S!B2:(C5:D3))", 12.),
        ("SUM(S!D3:C5:B2)", 12.),
        ("SUM((S!A1,A5):B5)", 10.),
        ("SUM(B5:(S!A1,A5))", 10.),
        ("SUM((A1,C3):(B2,D4))", 16.),
        ("SUM((D4,B2):(C3,A1))", 16.),
        ("SUM((A1:B3,B2:C4):D2)", 16.),
        ("SUM(D2:(A1:B3,B2:C4))", 16.),
        ("SUM((A1,A1):B2)", 4.),
        ("SUM((A1,B5):IF(TRUE,C3,D4))", 15.),
        // Negative OFFSET normalization is compatibility policy, native Excel unverified.
        ("ROWS((OFFSET(B3,0,0,-2,-2),A1):C4)", 4.),
        ("COLUMNS(C4:(A1,OFFSET(B3,0,0,-2,-2)))", 3.),
        ("ROW((OFFSET(B3,0,0,-2,-2),A1):C4)", 1.),
        ("COLUMN(C4:(A1,OFFSET(B3,0,0,-2,-2)))", 1.),
    ] {
        assert_eq!(
            value(&w, 0, source),
            CellValue::Number(expected),
            "{source}"
        );
    }
    for (source, expected) in [
        ("ROWS((A1,Other!A5):B5)", CellError::Ref),
        ("ROWS(B5:(Other!A1,A5))", CellError::Ref),
        ("ROWS((OFFSET(B3,0,0,-2,-2),Other!A1):C4)", CellError::Ref),
        ("ROWS((IF(TRUE,1/0,A1),A5):B5)", CellError::Div0),
        (
            "ROWS(IF(TRUE,1/0,A1):IF(TRUE,SQRT(-1),B1))",
            CellError::Div0,
        ),
        ("ROWS((A1,A5):ABS(1))", CellError::Value),
    ] {
        assert_eq!(value(&w, 0, source), CellValue::Error(expected), "{source}");
    }
    assert_eq!(value(&w, 0, "INDEX((F1,F2):G3,1,1)"), CellValue::Blank);
    assert_eq!(
        value(&w, 0, "ISBLANK(INDEX((F1,F2):G3,1,1))"),
        CellValue::Boolean(true)
    );
}
#[test]
fn chained_cells_and_valid_cell_named_sheet_spans_remain_distinct() {
    for exists in [false, true] {
        let w = book(exists);
        let sheet = usize::from(exists);
        assert_eq!(
            value(&w, sheet, "SUM(S!B2:C5:S!D3)"),
            CellValue::Number(12.)
        );
        assert_eq!(
            value(&w, sheet, "SUM((S!B2:C5):S!D3)"),
            CellValue::Number(12.)
        );
        if !exists {
            // Preserve the established pinned missing-first-sheet fallback for
            // actual sheet-span atoms; Calc returns NAME for these controls.
            for source in ["SUM(C5:S!D3)", "SUM('C5:S'!D3)"] {
                assert_eq!(value(&w, sheet, source), CellValue::Number(1.), "{source}");
            }
        }
        if exists {
            for source in ["SUM(C5:S!D3)", "SUM('C5:S'!D3)"] {
                assert_eq!(
                    value(&w, sheet, source),
                    CellValue::Number(101.),
                    "{source}"
                );
            }
            // Project cross-sheet colon guard differs from Calc's 106 here.
            assert_eq!(
                value(&w, sheet, "SUM(S!B2:(C5:S!D3))"),
                CellValue::Error(CellError::Ref)
            );
        }
    }
}
#[test]
fn envelope_membership_and_retained_dependents_refresh_after_edits() {
    let mut w = book(false);
    for (address, formula) in [("P1", "SUM((A1,C3):B2)"), ("Q1", "P1+1")] {
        w.sheet_mut(0)
            .unwrap()
            .set_formula(address.parse().unwrap(), formula)
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let context = EvaluationContext::default();
    let target = overlay.target("S!Q1").unwrap();
    for (edit, expected) in [
        (None, 10.),
        (Some(("B2", 8.)), 17.),
        (Some(("C3", 9.)), 25.),
    ] {
        if let Some((address, value)) = edit {
            let cell = overlay.target(&format!("S!{address}")).unwrap();
            overlay.set_value(cell, value).unwrap();
        }
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(target).unwrap(),
            CellValue::Number(expected)
        );
    }
    let inside = overlay.target("S!C2").unwrap();
    overlay.remove_cell(inside).unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(target).unwrap(),
        CellValue::Number(24.)
    );
    overlay.set_value(inside, 8.).unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(target).unwrap(),
        CellValue::Number(32.)
    );
    let unrelated = overlay.target("S!Z3").unwrap();
    overlay.set_value(unrelated, 99.).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(target).unwrap(), CellValue::Number(32.));
    assert_eq!(evaluation.usage().formula_executions, 0);
}
#[test]
fn envelope_construction_is_lazy_and_resource_failures_escape_masks() {
    let w = book(false);
    for (source, expected) in [
        ("ROWS((A1,A1048576):XFD1)", 1048576.),
        ("COLUMNS((A1,A1048576):XFD1)", 16384.),
    ] {
        let result = run_formula(
            &w,
            0,
            "Z100".parse().unwrap(),
            source,
            Default::default(),
            EvaluationBudgets {
                materialized_values: 3,
                ..Default::default()
            },
        );
        match result {
            EvaluationOutcome::Complete { value, usage } => {
                assert_eq!(value, CellValue::Number(expected));
                assert_eq!(usage.cell_reads, 0);
                assert_eq!(usage.sparse_entries, 0);
            }
            other => panic!("{source}: {other:?}"),
        }
    }
    assert!(matches!(
        run_formula(
            &w,
            0,
            "Z100".parse().unwrap(),
            "IFERROR(ROWS((A1,A2,A3):B5),7)",
            Default::default(),
            EvaluationBudgets {
                materialized_values: 2,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
