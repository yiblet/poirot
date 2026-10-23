//! Independently authored contracts from Microsoft documentation, retrieved 2026-10-21.
//! https://support.microsoft.com/en-us/excel/functions/tocol-function
//! https://support.microsoft.com/en-us/excel/functions/torow-function
//! https://support.microsoft.com/en-us/excel/functions/wrapcols-function
//! https://support.microsoft.com/en-us/excel/functions/wraprows-function
//! Empty results: https://support.microsoft.com/en-us/excel/how-to-correct-a-calc-error
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellError, CellValue, Workbook};
fn at(s: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: s.parse().unwrap(),
    }
}
fn policy() -> OverlayPolicy {
    OverlayPolicy {
        builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
        ..Default::default()
    }
}
fn formula(text: &str) -> CellValue {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    match run_formula_with_configuration(
        &book,
        0,
        at("Z100").address,
        text,
        &RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation),
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}

#[test]
fn scan_order_wrap_orientation_and_default_padding() {
    for (f, expected) in [
        ("INDEX(TOCOL({2,5;7,11}),2)", 5.),
        ("INDEX(TOCOL({2,5;7,11},0,TRUE),2)", 7.),
        ("INDEX(TOROW({2,5;7,11}),1,3)", 7.),
        ("INDEX(TOROW({2,5;7,11},,TRUE),1,3)", 5.),
        ("INDEX(WRAPCOLS({2,5,7,11,13},2),2,2)", 11.),
        ("INDEX(WRAPROWS({2;5;7;11;13},2),2,2)", 11.),
        ("INDEX(WRAPROWS({2,5,7},2,19),2,2)", 19.),
        ("ROWS(WRAPCOLS({2,5,7},99))", 3.),
        ("COLUMNS(WRAPROWS({2;5;7},99))", 3.),
        ("COLUMNS(WRAPROWS({2;5;7},1E100))", 3.),
        ("ROWS(WRAPCOLS({2,5,7},1E100))", 3.),
        ("SUM(TOCOL({2,#N/A;#DIV/0!,5},2))", 7.),
        ("SUM(TOROW({2,#N/A;#DIV/0!,5},3,TRUE))", 7.),
        ("SUM(WRAPROWS(TOROW({2,5;7,11}),2))", 25.),
    ] {
        assert_eq!(formula(f), CellValue::Number(expected), "{f}");
    }
    for (f, error) in [
        ("INDEX(WRAPCOLS({2,5,7},2),2,2)", CellError::NA),
        ("INDEX(WRAPROWS({2,5,7},2,#REF!),2,2)", CellError::Ref),
        ("INDEX(TOCOL({2,#DIV/0!}),2)", CellError::Div0),
        ("WRAPCOLS({2,5;7,11},2)", CellError::Value),
        ("WRAPROWS({2,5},0)", CellError::Num),
        ("WRAPCOLS({2,5},-2)", CellError::Num),
        ("TOCOL({2,5},4)", CellError::Value),
        ("TOROW({2,5},-1)", CellError::Value),
        ("TOCOL({2,5},1E100)", CellError::Value),
        ("TOCOL({2,5},-1E100)", CellError::Value),
        ("TOCOL({2,5},0,#REF!)", CellError::Ref),
        ("TOCOL({#N/A,#REF!},2)", CellError::Calc),
        ("TOCOL()", CellError::Value),
        ("TOROW(1,0,FALSE,9)", CellError::Value),
        ("WRAPROWS(1)", CellError::Value),
        ("WRAPCOLS(1,1,0,9)", CellError::Value),
        ("WRAPROWS({2,5},)", CellError::Num),
        ("WRAPROWS({2,5},#DIV/0!)", CellError::Div0),
        ("TOCOL({2,5},#N/A)", CellError::NA),
    ] {
        assert_eq!(formula(f), CellValue::Error(error), "{f}");
    }
}
#[test]
fn blanks_are_filtered_before_normalization_but_empty_text_and_zero_survive() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("B1").address, "\"\"").unwrap();
    sheet.set_value(at("A2").address, 0.).unwrap();
    sheet
        .set_value(at("B2").address, CellValue::Error(CellError::NA))
        .unwrap();
    for (cell, f) in [
        ("D1", "TOCOL(A1:B2)"),
        ("F1", "TOCOL(A1:B2,1)"),
        ("H1", "TOCOL(A1:B2,2)"),
        ("J1", "TOCOL(A1:B2,3,TRUE)"),
        ("L1", "TOROW(A1:B2,3)"),
    ] {
        sheet.set_formula(at(cell).address, f).unwrap();
    }
    // The true blank is omitted, while the formula empty text remains a member.
    sheet.set_formula(at("P1").address, "TOCOL(A1,1)").unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    for (cell, value) in [
        ("P1", CellValue::Error(CellError::Calc)),
        ("D1", 0.0.into()),
        ("D2", CellValue::Text("".into())),
        ("D3", 0.0.into()),
        ("D4", CellValue::Error(CellError::NA)),
        ("F1", CellValue::Text("".into())),
        ("F2", 0.0.into()),
        ("F3", CellValue::Error(CellError::NA)),
        ("F4", CellValue::Blank),
        ("H1", 0.0.into()),
        ("H2", CellValue::Text("".into())),
        ("H3", 0.0.into()),
        ("H4", CellValue::Blank),
        ("J1", 0.0.into()),
        ("J2", CellValue::Text("".into())),
        ("J3", CellValue::Blank),
        ("L1", CellValue::Text("".into())),
        ("M1", 0.0.into()),
    ]
    .into_iter()
    .rev()
    {
        assert_eq!(direct.cell(at(cell)).unwrap(), value, "direct {cell}");
        assert_eq!(run.cell(at(cell)).unwrap(), value, "overlay {cell}");
    }
}
#[test]
fn overlay_edits_change_filtered_spill_and_wrapped_shape() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 2.).unwrap();
    sheet.set_value(at("A3").address, 7.).unwrap();
    sheet
        .set_formula(at("D1").address, "TOCOL(A1:A3,1)")
        .unwrap();
    sheet
        .set_formula(at("G1").address, "WRAPROWS({2,5,7},J1,0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (edit, width, child, wrap) in [
        (None, 2, CellValue::Blank, 0.0.into()),
        (Some(5.), 3, 7.0.into(), CellValue::Blank),
        (None, 2, CellValue::Blank, 0.0.into()),
    ] {
        if let Some(n) = edit {
            overlay.set_value(at("A2"), n).unwrap();
        } else {
            overlay.set_value(at("A2"), CellValue::Blank).unwrap();
        }
        overlay.set_value(at("J1"), width as f64).unwrap();
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(at("D3")).unwrap(), child);
        assert_eq!(run.cell(at("H2")).unwrap(), wrap);
    }
}
#[test]
fn blocked_spills_grid_limits_and_operational_budgets() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("D1").address, "TOCOL({2,5})").unwrap();
    sheet.set_value(at("D2").address, 9.).unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct.cell(at("D1")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
    assert_eq!(
        formula("TOROW(SEQUENCE(16385))"),
        CellValue::Error(CellError::Num)
    );
    assert_eq!(
        formula("INDEX(TOROW(SEQUENCE(16384)),1,16384)"),
        16384.0.into()
    );
    assert!(matches!(
        run_formula_with_configuration(
            &book,
            0,
            at("Z100").address,
            "IFERROR(SUM(TOCOL(SEQUENCE(1000))),99)",
            &RunConfiguration::default()
                .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation),
            EvaluationBudgets {
                materialized_values: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn mixed_types_and_explicit_blank_optional_arguments_remain_distinct() {
    for (f, expected) in [
        (
            "INDEX(TOCOL({TRUE,\"green\";2,5},,TRUE),3)",
            CellValue::Text("green".into()),
        ),
        (
            "INDEX(TOROW({TRUE,\"green\";2,5}),1,1)",
            CellValue::Boolean(true),
        ),
        (
            "INDEX(WRAPROWS({TRUE,\"green\",2},2),1,2)",
            CellValue::Text("green".into()),
        ),
        (
            "INDEX(WRAPCOLS({TRUE,\"green\",2},2),2,1)",
            CellValue::Text("green".into()),
        ),
        (
            "ISNUMBER(INDEX(WRAPROWS({2,5,7},2,A1),2,2))",
            CellValue::Boolean(true),
        ),
        ("INDEX(TOROW({2,5;7,11},A1,A1),1,2)", CellValue::Number(5.)),
        ("INDEX(TOROW({2,5;7,11},,),1,2)", CellValue::Number(5.)),
    ] {
        assert_eq!(formula(f), expected, "{f}");
    }
}
