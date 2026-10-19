//! DAVERAGE finalization is opt-in; database candidate admission stays pinned.
use poirot_eval::{
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellError, CellValue, Workbook};

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
        ("A1", "N".into()),
        ("A2", 0.0.into()),
        ("A3", 2.0.into()),
        ("A4", (-2.0).into()),
        ("B1", "N".into()),
        ("B2", "text".into()),
        ("C1", "N".into()),
        ("C2", CellValue::Error(CellError::NA)),
        ("E1", "N".into()),
        ("F1", "N".into()),
        ("E2", ">5".into()),
    ] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    book
}
#[test]
fn database_average_distinguishes_empty_samples_from_zero_values() {
    let book = book();
    for policy in [
        BuiltinSemanticsPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation,
    ] {
        let config = RunConfiguration::default().with_builtin_policy(policy);
        let empty = if policy == BuiltinSemanticsPolicy::ExcelDocumentation {
            CellValue::Error(CellError::Div0)
        } else {
            0.0.into()
        };
        for (formula, expected) in [
            ("DAVERAGE(A1:A1,1,E1:E2)", empty.clone()),
            ("DAVERAGE(A1:A4,1,E1:E2)", empty.clone()),
            ("DAVERAGE(B1:B3,1,E1:E1)", empty.clone()),
            ("DAVERAGE(B1:B3,1,F1:F2)", empty.clone()),
            ("DAVERAGE(A1:A2,1,F1:F2)", 0.0.into()),
            ("DAVERAGE(A1:A4,1,F1:F2)", 0.0.into()),
            // Deliberately preserved candidate-error behavior; Calc returns #N/A.
            ("DAVERAGE(C1:C2,1,F1:F2)", empty.clone()),
            ("DCOUNT(A1:A4,1,E1:E2)", 0.0.into()),
            ("DSUM(A1:A4,1,E1:E2)", 0.0.into()),
            ("DMIN(A1:A4,1,E1:E2)", 0.0.into()),
            ("DMAX(A1:A4,1,E1:E2)", 0.0.into()),
            (
                "DAVERAGE(A1:A4,NA(),E1:E2)",
                CellValue::Error(CellError::NA),
            ),
        ] {
            let result = run_formula_with_configuration(
                &book,
                0,
                target("Z1").address,
                formula,
                &config,
                EvaluationBudgets::default(),
            );
            assert!(
                matches!(&result, EvaluationOutcome::Complete {value, ..} if value == &expected),
                "{policy:?} {formula}: {result:?}"
            );
        }
        for budgets in [
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            },
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                run_formula_with_configuration(
                    &book,
                    0,
                    target("Z1").address,
                    "IFERROR(DAVERAGE(A1:A4,1,E1:E2),99)",
                    &config,
                    budgets
                ),
                EvaluationOutcome::Aborted { .. }
            ));
        }
    }
}
#[test]
fn database_policy_and_criteria_edits_invalidate_retained_results() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("Z1").address,
            "DAVERAGE(A1:A4,1,E1:E2)",
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(poirot_eval::overlay::OverlayPolicy {
            builtin: poirot_eval::BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        0.0.into()
    );
    overlay
        .set_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        CellValue::Error(CellError::Div0)
    );
    overlay.set_value(target("E2"), "=0").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        0.0.into()
    );
    overlay.set_value(target("E2"), ">5").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        CellValue::Error(CellError::Div0)
    );
    overlay
        .set_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::PinnedPoi,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        0.0.into()
    );
}
