//! Whole-row reference traversal preserves effective source order and admission.
//! SUM adds referenced numbers: https://support.microsoft.com/en-us/excel/functions/sum-function
//! Exact floating-point order and admission limits are Poirot contracts.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetKind, EvaluationFailure};
use crate::eval::overlay::{Evaluation, Overlay};
use crate::model::{CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn whole_rows_keep_order_and_effective_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, value) in [
        ("A2", 1e16),
        ("XFD2", 1.),
        ("A3", -1e16),
        ("XFD3", 2.),
        ("A4", 99.),
    ] {
        sheet.set_value(at(address).address, value).unwrap();
    }
    sheet.set_formula(at("A1").address, "SUM(2:3)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    for step in 0..3 {
        if step == 1 {
            direct.remove_cell(at("XFD2")).unwrap();
            overlay.remove_cell(at("XFD2")).unwrap();
            direct.set_value(at("XFD3"), 5.).unwrap();
            overlay.set_value(at("XFD3"), 5.).unwrap();
        } else if step == 2 {
            direct.set_value(at("XFD2"), "ignored").unwrap();
            overlay.set_value(at("XFD2"), "ignored").unwrap();
            direct.set_value(at("XFD3"), true).unwrap();
            overlay.set_value(at("XFD3"), true).unwrap();
        }
        let expected = CellValue::Number([2., 5., 0.][step]);
        assert_eq!(direct.cell(at("A1")).unwrap(), expected);
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("A1"))
                .unwrap(),
            expected
        );
    }
}

#[test]
fn whole_row_collection_refuses_before_evaluating_referenced_errors() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "SUM(2:3)").unwrap();
    sheet.set_formula(at("A2").address, "1/0").unwrap();
    for address in ["XFD2", "A3", "XFD3"] {
        sheet.set_value(at(address).address, 1.).unwrap();
    }
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_collection_values(3);
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    assert!(
        matches!(direct.cell(at("A1")), Err(EvaluationFailure::Budget(failure)) if failure.kind == BudgetKind::Materialization)
    );
    assert!(
        matches!(overlay.evaluation(&context).unwrap().cell(at("A1")), Err(EvaluationFailure::Budget(failure)) if failure.kind == BudgetKind::Materialization)
    );
}
