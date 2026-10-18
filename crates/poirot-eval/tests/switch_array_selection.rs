//! First-match array selection follows the existing lazy argument contract.
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
fn first_match_default_and_singleton_axes_select_independent_positions() {
    // Corroborated independently in isolated Calc snapshots, excluding typed-coercion controls.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("SUM(SWITCH({1,2,1},1,{3,100,7},2,5,11))", 15.),
            ("SUM(SWITCH({1,3,2},1,4,1,99,2,5,9))", 18.),
            ("SUM(SWITCH({1;2},1,{1,2,3},9))", 33.),
            ("SUM(SWITCH(1,{1,2},2,3))", 5.),
            ("SUM(SWITCH(1,0,100,{1,2},2,3))", 5.),
            ("SUM(SWITCH({1,2},{1;2},2,3))", 10.),
            ("SUM(SWITCH({1,2},1,2,{4,8}))", 10.),
            ("SUM(SWITCH({1,2},1,2,{4;8}))", 16.),
            ("SUMPRODUCT(SWITCH(C1:C3,TRUE,A1:A3,FALSE,B1:B3))", 6.),
            ("COUNT(SWITCH({1,2},1,J1,2,3))", 1.),
            (r#"SUM(SWITCH({"a","A"},"a",2,9))"#, 4.),
        ] {
            assert_eq!(
                value(result(&book(), formula, arrays)),
                CellValue::Number(expected),
                "{arrays:?} {formula}"
            );
        }
        assert_eq!(
            value(result(&book(), "SUM(SWITCH({1,2},1,2))", arrays)),
            CellValue::Error(CellError::NA)
        );
    }
}
#[test]
fn scalar_identity_omission_and_blank_are_preserved() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("ISREF(SWITCH(1,1,A1:B3,0))", CellValue::Boolean(true)),
            ("ISREF(SWITCH({1},1,A1:A3,0))", CellValue::Boolean(false)),
            ("ISBLANK(SWITCH(1,1,))", CellValue::Boolean(false)),
            ("ISBLANK(SWITCH(1,1,J1))", CellValue::Boolean(true)),
            (
                "AND(ISBLANK(SWITCH({1,2},1,J1,2,J1)))",
                CellValue::Boolean(true),
            ),
            ("SWITCH()", CellValue::Error(CellError::NA)),
            ("SWITCH(1,1)", CellValue::Error(CellError::NA)),
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
fn scalar_comparison_and_error_policies_apply_to_array_members() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("D1").address, "1/0", CellValue::Blank)
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(target("E1").address, "NA()", CellValue::Blank)
        .unwrap();
    for (formula, expected) in [
        // Current typed comparison intentionally differs from Calc's broader SWITCH admission.
        (
            r#"SUM(SWITCH({"1",1,TRUE},1,2,TRUE,3,9))"#,
            CellValue::Number(14.),
        ),
        ("SUM(SWITCH({0,1},J1,2,9))", CellValue::Number(11.)),
        (
            "SUM(SWITCH({2.9999999999999996,3},3,2,9))",
            CellValue::Number(4.),
        ),
        ("SWITCH(1/0,1,2,9)", CellValue::Error(CellError::NA)),
        ("SWITCH(1,D1,2,9)", CellValue::Error(CellError::NA)),
        (
            "SUMPRODUCT(SWITCH(D1:E1,1,2,9))",
            CellValue::Error(CellError::NA),
        ),
        (
            "SUM(SWITCH({1,2},1,2,D1,3,9))",
            CellValue::Error(CellError::NA),
        ),
        (
            "SUM(SWITCH({1,2},1,2,2,1/0))",
            CellValue::Error(CellError::Div0),
        ),
    ] {
        assert_eq!(
            value(result(
                &book,
                formula,
                ArrayResultPolicy::ScalarIntersection
            )),
            expected,
            "{formula}"
        );
    }
    assert!(matches!(
        result(
            &book,
            r#"IFERROR(SUM(SWITCH({"é","a"},"a",2,9)),99)"#,
            ArrayResultPolicy::ScalarIntersection
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn later_cases_and_unselected_reference_members_are_not_read() {
    let mut book = book();
    for (cell, formula) in [
        ("F1", "1"),
        ("G1", "G1"),
        ("H1", "H1"),
        ("I1", "2"),
        ("K1", "COUNTIF(A1:A3,B1:B2)"),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target(cell).address, formula, CellValue::Blank)
            .unwrap();
    }
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("SUM(SWITCH({1,2},1,F1:G1,2,H1:I1))", 3.),
            ("SUM(SWITCH({1,1},1,2,K1,3,9))", 4.),
            ("SUM(SWITCH({1,1},1,2,2,K1,9))", 4.),
            ("SUM(SWITCH({1,1},1,2,K1))", 4.),
            ("SUM(SWITCH({1,2},1,2,2,3,K1))", 5.),
        ] {
            assert_eq!(
                value(result(&book, formula, arrays)),
                CellValue::Number(expected),
                "{formula}"
            );
        }
        for formula in [
            "IFERROR(SUM(SWITCH({1,2},1,2,K1,3,9)),99)",
            "IFERROR(SUM(SWITCH({1,2},1,K1,2,3)),99)",
            "IFERROR(SUM(SWITCH({1,2},1,2,K1)),99)",
        ] {
            assert!(
                matches!(
                    result(&book, formula, arrays),
                    EvaluationOutcome::Aborted { .. }
                ),
                "{formula}"
            );
        }
    }
}
#[test]
fn needed_branches_execute_once_and_resource_limits_remain_operational() {
    let outcome = result(
        &book(),
        "SUM(SWITCH({1,2,1},1,RAND(),2,RAND(),RAND()))",
        ArrayResultPolicy::ScalarIntersection,
    );
    assert_eq!(outcome.usage().random_accesses.first().unwrap().draws, 2);
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
                "IFERROR(SUM(SWITCH({1;2},1,{1,2,3},9)),99)",
                &config(ArrayResultPolicy::ScalarIntersection),
                budget
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn incompatible_extents_are_rejected_without_unused_shape_scans() {
    for formula in [
        "SUM(SWITCH({1,2},1,{1,2,3},9))",
        "SUM(SWITCH({1,2,1},1,{1,2},9))",
        "SUM(SWITCH({1,2},1,2,{1,2,3},9))",
    ] {
        assert_eq!(
            value(result(
                &book(),
                formula,
                ArrayResultPolicy::ScalarIntersection
            )),
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    // Retained lazy contract differs from Calc's unused-operand shape scanning.
    assert_eq!(
        value(result(
            &book(),
            "SUM(SWITCH({1,1},1,2,3,{10,20,30},9))",
            ArrayResultPolicy::ScalarIntersection
        )),
        CellValue::Number(4.)
    );
}
#[test]
fn policy_and_expression_edits_invalidate_results_after_snapshot_restore() {
    let mut book = book();
    for (cell, formula) in [
        ("E1", "SUM(SWITCH({1,2,1},1,{3,100,7},2,5,11))"),
        ("E3", "SUMPRODUCT(SWITCH(C1:C3,TRUE,A1:A3,FALSE,B1:B3))"),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target(cell).address, formula, CellValue::Blank)
            .unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E1"))
            .unwrap(),
        CellValue::Error(CellError::NA)
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
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn expression_case_and_result_dimensions_bound_child_first_spills() {
    for formula in [
        "SWITCH({1;2},1,{1,2,3},{10,20,30})",
        "SWITCH(1,{1;2},{1,2,3},{10,20,30})",
    ] {
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target("E5").address, formula, CellValue::Blank)
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
                "{formula}: {cell}"
            );
        }
    }
}
