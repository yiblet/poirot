//! Equal-shaped SUMPRODUCT arguments request the same text/blank admission.
use poirot::eval::{
    overlay::{EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, Workbook};
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, value) in [
        ("A1", CellValue::Text("4".into())),
        ("A2", CellValue::Text("x".into())),
        (
            "A3",
            CellValue::TextUtf16(poirot::model::ExcelText::from("4")),
        ),
        (
            "A4",
            CellValue::TextUtf16(poirot::model::ExcelText::from_units(vec![0xd800])),
        ),
        ("B1", CellValue::Number(2.)),
        ("B2", CellValue::Number(3.)),
        ("C1", CellValue::Error(CellError::NA)),
    ] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    book
}
fn result(
    formula: &str,
    policy: BuiltinSemanticsPolicy,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        &book(),
        0,
        target("Z100").address,
        formula,
        &RunConfiguration::default().with_builtin_policy(policy),
        budgets,
    )
}
fn value(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
#[test]
fn singleton_text_and_blank_admission_is_independent_of_argument_origin_and_order() {
    for argument in [
        r#""4""#, r#""x""#, r#""""#, "A1", "A1:A1", "A3", "A3:A3", r#"{"4"}"#, "D1", "D1:D1",
    ] {
        for formula in [
            format!("SUMPRODUCT({argument})"),
            format!("SUMPRODUCT({argument},2)"),
            format!("SUMPRODUCT(2,{argument})"),
            format!("SUMPRODUCT({argument},B1:B1)"),
            format!("SUMPRODUCT(B1:B1,{argument})"),
        ] {
            assert_eq!(
                value(result(
                    &formula,
                    BuiltinSemanticsPolicy::ExcelDocumentation,
                    EvaluationBudgets::default()
                )),
                CellValue::Number(0.),
                "{formula}"
            );
        }
    }
    for (formula, expected) in [
        ("SUMPRODUCT(A1:A2,B1:B2)", 0.),
        ("SUMPRODUCT(B1:B2,A1:A2)", 0.),
        ("SUMPRODUCT(--A1:A1,2)", 8.),
        ("SUMPRODUCT(2,--A1:A1)", 8.),
        ("SUMPRODUCT(4,B1)", 8.),
        ("SUMPRODUCT(B1:B1,4)", 8.),
    ] {
        assert_eq!(
            value(result(
                formula,
                BuiltinSemanticsPolicy::ExcelDocumentation,
                EvaluationBudgets::default()
            )),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}
#[test]
fn conformability_and_errors_remain_explicit_and_operational_failures_escape() {
    for (formula, expected) in [
        ("SUMPRODUCT(B1:B2,2)", CellError::Value),
        ("SUMPRODUCT(2,B1:B2)", CellError::Value),
        ("SUMPRODUCT(C1:C1,B1:B2)", CellError::Value),
        ("SUMPRODUCT(B1:B2,C1:C1)", CellError::Value),
        ("SUMPRODUCT(C1:C1,2)", CellError::NA),
        ("SUMPRODUCT(2,C1)", CellError::NA),
        ("SUMPRODUCT(NA())", CellError::NA),
        // Omitted arguments retain their existing admission, distinct from blank.
        ("SUMPRODUCT(,2)", CellError::Value),
        ("SUMPRODUCT(2,)", CellError::Value),
    ] {
        assert_eq!(
            value(result(
                formula,
                BuiltinSemanticsPolicy::ExcelDocumentation,
                EvaluationBudgets::default()
            )),
            CellValue::Error(expected),
            "{formula}"
        );
    }
    // Excluded text requires no UTF-16 projection, including malformed units.
    assert_eq!(
        value(result(
            "SUMPRODUCT(A4)",
            BuiltinSemanticsPolicy::ExcelDocumentation,
            EvaluationBudgets::default()
        )),
        CellValue::Number(0.)
    );
    assert!(matches!(
        result(
            "IFERROR(SUMPRODUCT(A1:A2,B1:B2),99)",
            BuiltinSemanticsPolicy::ExcelDocumentation,
            EvaluationBudgets {
                run_work: 0,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn singleton_source_edits_invalidate_results_after_snapshot() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("F1").address, "SUMPRODUCT(A1:A1,B1:B1)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(poirot::eval::overlay::EvaluationPolicy {
            builtin: poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        CellValue::Number(0.)
    );
    let (restored, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut overlay = state.attach(&restored).unwrap();
    overlay.set_value(target("A1"), 4.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        CellValue::Number(8.)
    );
    overlay.set_value(target("A1"), "4").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("F1"))
            .unwrap(),
        CellValue::Number(0.)
    );
}
