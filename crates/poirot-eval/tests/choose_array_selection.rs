//! Positional index selection follows the existing lazy argument contract.
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
fn independent_indices_truncation_and_singleton_axes_select_branch_positions() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("SUM(CHOOSE({1,2,1},{3,100,7},5))", 15.),
            ("SUM(CHOOSE({1;2},{1,2,3},9))", 33.),
            ("SUM(CHOOSE({1,2},2,{4;8}))", 16.),
            ("SUM(CHOOSE({1.9,2.9},2,3))", 5.),
            ("SUM(CHOOSE({1,1},2,1/0))", 4.),
            ("SUM(CHOOSE({1,2},,3))", 3.),
            ("SUM(CHOOSE({1,2,3},7,11,13))", 31.),
            ("COUNT(CHOOSE({1,2},J1,3))", 1.),
        ] {
            assert_eq!(
                value(result(&book(), formula, arrays)),
                CellValue::Number(expected),
                "{formula}"
            );
        }
        for (formula, expected) in [
            ("ISREF(CHOOSE(1,A1:A3,0))", CellValue::Boolean(true)),
            ("ISREF(CHOOSE({1},A1:A3,0))", CellValue::Boolean(false)),
            ("ISBLANK(CHOOSE(1,))", CellValue::Boolean(true)),
            ("ISBLANK(CHOOSE(1,J1))", CellValue::Boolean(true)),
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
fn index_members_reuse_scalar_admission_and_preserve_individual_errors() {
    let mut book = book();
    for (cell, v) in [
        ("D1", CellValue::Number(f64::INFINITY)),
        ("D2", CellValue::Number(f64::NAN)),
        ("E1", CellValue::Error(CellError::Div0)),
    ] {
        book.sheet_mut(0)
            .unwrap()
            .set_value(target(cell).address, v)
            .unwrap();
    }
    // Existing scalar admission deliberately accepts boolean and numeric text indices,
    // unlike Calc's array boolean admission. Both origins share one policy here.
    for (formula, expected) in [
        (r#"SUM(CHOOSE({"1",TRUE,2.8},2,3))"#, CellValue::Number(7.)),
        ("SUM(CHOOSE({0,1},2,3))", CellValue::Error(CellError::Value)),
        ("SUM(CHOOSE({3,1},2,3))", CellValue::Error(CellError::Value)),
        (
            "SUM(CHOOSE({1,2},2,1/0))",
            CellValue::Error(CellError::Div0),
        ),
        (
            "SUMPRODUCT(CHOOSE(D1:D2,2,3))",
            CellValue::Error(CellError::Num),
        ),
        ("CHOOSE(E1,2,3)", CellValue::Error(CellError::Div0)),
        (
            "SUMPRODUCT(CHOOSE(J1:J2,2,3))",
            CellValue::Error(CellError::Value),
        ),
        (
            "SUMPRODUCT(--ISERROR(CHOOSE({0,1,3},2,3)))",
            CellValue::Number(2.),
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
}
#[test]
fn unselected_branches_and_reference_members_are_not_read() {
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
            ("SUM(CHOOSE({1,2},F1:G1,H1:I1))", 3.),
            ("SUM(CHOOSE({1,1},2,K1))", 4.),
        ] {
            assert_eq!(
                value(result(&book, formula, arrays)),
                CellValue::Number(expected),
                "{formula}"
            );
        }
        assert!(matches!(
            result(&book, "IFERROR(SUM(CHOOSE({1,2},2,K1)),99)", arrays),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn selected_branches_execute_once_and_budgets_remain_operational() {
    let outcome = result(
        &book(),
        "SUM(CHOOSE({1,2,1},RAND(),RAND(),RAND()))",
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
                "IFERROR(SUM(CHOOSE({1;2},{1,2,3},9)),99)",
                &config(ArrayResultPolicy::ScalarIntersection),
                budget
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn incompatible_extents_reject_without_unused_shape_scanning() {
    for formula in [
        "SUM(CHOOSE({1,2},2,{4,8,16}))",
        "SUM(CHOOSE({1,2,1},2,{4,8}))",
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
            "SUM(CHOOSE({1,1},2,{4;8}))",
            ArrayResultPolicy::ScalarIntersection
        )),
        CellValue::Number(4.)
    );
}
#[test]
fn policy_and_index_edits_invalidate_restored_results() {
    let mut book = book();
    for (cell, formula) in [
        ("E1", "SUM(CHOOSE({1,2,1},{3,100,7},5))"),
        ("E3", "SUMPRODUCT(CHOOSE(C1:C3,A1:A3,B1:B3))"),
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
        CellValue::Number(110.)
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
    // TRUE -> 1, FALSE -> 0 follows existing scalar CHOOSE, so use numeric index references.
    overlay.set_value(target("C1"), 1.).unwrap();
    overlay.set_value(target("C2"), 2.).unwrap();
    overlay.set_value(target("C3"), 1.).unwrap();
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
    overlay.set_value(target("C1"), 2.).unwrap();
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
        CellValue::Number(110.)
    );
}
#[test]
fn index_and_branch_dimensions_bound_child_first_spills() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E5").address,
            "CHOOSE({1;2},{1,2,3},{10,20,30})",
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
