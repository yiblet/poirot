//! First-true array selection follows the existing lazy argument contract.
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
        ("A1", 1.),
        ("A2", 100.),
        ("A3", 3.),
        ("B1", 200.),
        ("B2", 2.),
        ("B3", 300.),
    ] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    for (cell, value) in [("C1", true), ("C2", false), ("C3", true)] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    book
}
fn config(arrays: ArrayResultPolicy) -> RunConfiguration {
    RunConfiguration::default()
        .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
        .with_array_result_policy(arrays)
}
fn result(
    book: &Workbook,
    formula: &str,
    arrays: ArrayResultPolicy,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        target("Z100").address,
        formula,
        &config(arrays),
        EvaluationBudgets::default(),
    )
}
fn value(result: EvaluationOutcome<CellValue>) -> CellValue {
    match result {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
#[test]
fn first_true_positions_and_singleton_axes_are_independently_selected() {
    // Values independently observed in isolated Calc snapshots; see evidence report.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("SUM(IFS({TRUE,FALSE,TRUE},{3,100,7},TRUE,5))", 15.),
            (
                "SUM(IFS({TRUE,FALSE,TRUE},{3,100,7},{FALSE,TRUE,FALSE},{20,5,30},TRUE,99))",
                15.,
            ),
            ("SUM(IFS(FALSE,1,{TRUE,FALSE},2,TRUE,3))", 5.),
            (
                "SUM(IFS({TRUE,FALSE;FALSE,TRUE},{1,2;3,4},TRUE,{10,20;30,40}))",
                55.,
            ),
            ("SUM(IFS({TRUE;FALSE},{1,2,3},TRUE,9))", 33.),
            ("SUM(IFS({TRUE,FALSE},2,{FALSE;TRUE},9,TRUE,7))", 20.),
            ("SUM(IFS({1,0},2,TRUE,3))", 5.),
            (r#"SUM(IFS({"TRUE","FALSE"},2,TRUE,3))"#, 5.),
            ("SUMPRODUCT(IFS(C1:C3,A1:A3,TRUE,B1:B3))", 6.),
            ("COUNT(IFS({TRUE,FALSE},J1,TRUE,3))", 1.),
        ] {
            assert_eq!(
                value(result(&book(), formula, arrays)),
                CellValue::Number(expected),
                "{arrays:?} {formula}"
            );
        }
        assert_eq!(
            value(result(&book(), "SUM(IFS({TRUE,FALSE},2))", arrays)),
            CellValue::Error(CellError::NA)
        );
    }
}
#[test]
fn reference_identity_omission_and_inline_blank_remain_distinct() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("ISREF(IFS(TRUE,A1:A3,TRUE,0))", CellValue::Boolean(true)),
            ("ISREF(IFS({TRUE},A1:A3,TRUE,0))", CellValue::Boolean(false)),
            (
                "AND(ISBLANK(IFS({TRUE,FALSE},J1,TRUE,J1)))",
                CellValue::Boolean(true),
            ),
            ("ISBLANK(IFS(TRUE,))", CellValue::Boolean(false)),
            ("ISBLANK(IFS(TRUE,J1))", CellValue::Boolean(true)),
            ("IFS(FALSE,1,FALSE,2)", CellValue::Error(CellError::NA)),
            ("IFS(TRUE)", CellValue::Error(CellError::Value)),
        ] {
            assert_eq!(
                value(result(&book(), formula, arrays)),
                expected,
                "{formula}"
            );
        }
    }
}
#[test]
fn only_unresolved_conditions_and_selected_reference_members_are_read() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    for (cell, formula) in [
        ("D1", "1/0"),
        ("E1", "NA()"),
        ("F1", "1"),
        ("G1", "G1"),
        ("H1", "H1"),
        ("I1", "2"),
        ("K1", "COUNTIF(A1:A3,B1:B2)"),
    ] {
        sheet
            .set_formula(target(cell).address, formula, CellValue::Blank)
            .unwrap();
    }
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        assert_eq!(
            value(result(
                &book,
                "SUM(IFS({TRUE,FALSE},F1:G1,TRUE,H1:I1))",
                arrays
            )),
            CellValue::Number(3.)
        );
        assert_eq!(
            value(result(&book, "SUM(IFS({TRUE,TRUE},2,K1,3))", arrays)),
            CellValue::Number(4.)
        );
        assert_eq!(
            value(result(
                &book,
                "SUM(IFS({TRUE,FALSE},2,D1:E1,3,TRUE,4))",
                arrays
            )),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            value(result(&book, "SUM(IFS({TRUE,FALSE},2,TRUE,1/0))", arrays)),
            CellValue::Error(CellError::Div0)
        );
        assert!(matches!(
            result(
                &book,
                "IFERROR(SUM(IFS({TRUE,FALSE},K1,TRUE,2)),99)",
                arrays
            ),
            EvaluationOutcome::Aborted { .. }
        ));
        assert!(matches!(
            result(&book, "IFERROR(SUM(IFS({TRUE,FALSE},2,K1,3)),99)", arrays),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn needed_compound_arguments_execute_once_and_limits_remain_operational() {
    let outcome = result(
        &book(),
        "SUM(IFS({TRUE,FALSE,TRUE},RAND(),TRUE,RAND(),TRUE,RAND()))",
        ArrayResultPolicy::ScalarIntersection,
    );
    assert_eq!(outcome.usage().random_accesses.first().unwrap().draws, 2);
    assert!(matches!(value(outcome),CellValue::Number(n) if (0.0..3.0).contains(&n)));
    for budget in [
        EvaluationBudgets {
            materialized_values: 5,
            ..Default::default()
        },
        EvaluationBudgets {
            run_work: 5,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run_formula_with_configuration(
                &book(),
                0,
                target("Z100").address,
                "IFERROR(SUM(IFS({TRUE;FALSE},{1,2,3},TRUE,9)),99)",
                &config(ArrayResultPolicy::ScalarIntersection),
                budget
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn incompatible_extents_are_rejected_without_emulating_calc_padding() {
    // Deliberate bounded contract: no guessed truncation, padding, or unused-shape scans.
    for formula in [
        "SUM(IFS({TRUE,FALSE,TRUE},{1,2},TRUE,9))",
        "SUM(IFS({TRUE,FALSE},2,{FALSE,TRUE,TRUE},9))",
    ] {
        assert_eq!(
            value(result(
                &book(),
                formula,
                ArrayResultPolicy::ScalarIntersection
            )),
            CellValue::Error(CellError::Value)
        );
    }
    assert_eq!(
        value(result(
            &book(),
            "SUM(IFS({TRUE,TRUE},2,TRUE,{10,20,30}))",
            ArrayResultPolicy::ScalarIntersection
        )),
        CellValue::Number(4.)
    );
}
#[test]
fn policy_and_condition_edits_invalidate_retained_results_after_restore() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E1").address,
            "SUM(IFS({TRUE,FALSE,TRUE},{3,100,7},TRUE,5))",
            CellValue::Blank,
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E3").address,
            "SUMPRODUCT(IFS(C1:C3,A1:A3,TRUE,B1:B3))",
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E1"))
            .unwrap(),
        CellValue::Error(CellError::Value)
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
            .cell(target("E1"))
            .unwrap(),
        CellValue::Number(15.)
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E3"))
            .unwrap(),
        CellValue::Number(6.)
    );
    let (restored, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut overlay = state.attach(&restored).unwrap();
    overlay.set_value(target("C1"), false).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E3"))
            .unwrap(),
        CellValue::Number(205.)
    );
    overlay.set_policy(OverlayPolicy::default()).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E1"))
            .unwrap(),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn condition_and_selected_result_dimensions_bound_child_first_spills() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E5").address,
            "IFS({TRUE;FALSE},{1,2,3},TRUE,{10,20,30})",
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(OverlayPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for (cell, expected) in [("G6", 30.), ("E5", 1.), ("F5", 2.), ("E6", 10.)] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            CellValue::Number(expected),
            "{cell}"
        );
    }
}
