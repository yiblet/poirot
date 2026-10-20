//! Positional IF selection shares rectangular traversal without changing pinned scalar semantics.
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
    for (cell, value) in [("A1", 1.), ("A3", 3.), ("B2", 2.)] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    for cell in ["A2", "B1", "B3"] {
        sheet.set_formula(target(cell).address, cell).unwrap();
    }
    book
}
fn config(arrays: ArrayResultPolicy) -> RunConfiguration {
    RunConfiguration::default()
        .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
        .with_array_result_policy(arrays)
}
fn evaluate(
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
fn if_selects_positions_and_broadcasts_only_singleton_condition_axes() {
    // Independently corroborated by isolated Calc snapshots, not Poirot output.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("SUM(IF({TRUE,FALSE,TRUE},2,{30,5,70}))", 9.),
            ("SUM(IF({TRUE;FALSE},{1,2,3},{10,20,30}))", 66.),
            ("SUM(IF({TRUE,FALSE},{1,2,3},9))", 10.),
            ("SUM(IF({TRUE,FALSE},{1;2;3},9))", 33.),
            ("SUM(IF({TRUE,TRUE},2,{10,20,30}))", 4.),
            ("SUM(IF(TRUE,{1,2,3},{10,20,30,40}))", 6.),
            ("SUM(IF({TRUE,TRUE},2,1/0))", 4.),
            (
                "SUM(IF({TRUE,FALSE,TRUE},IF({FALSE,TRUE,TRUE},{3,5,7},{11,13,17}),19))",
                37.,
            ),
            ("COUNT(IF({TRUE,FALSE},D1,9))", 1.),
            ("COUNT(IF({TRUE,FALSE},\"\",9))", 1.),
            ("COUNT(IF({TRUE,FALSE},0,9))", 2.),
            ("PRODUCT(IF({TRUE,FALSE,TRUE},{3,100,7}))", 21.),
            ("PRODUCT(IF({TRUE,FALSE,TRUE},{3,100,7},0))", 0.),
            ("PRODUCT(IF({TRUE,FALSE,TRUE},{3,100,7},\"\"))", 21.),
        ] {
            assert_eq!(
                value(evaluate(&book(), formula, arrays)),
                CellValue::Number(expected),
                "{arrays:?} {formula}"
            );
        }
        assert_eq!(
            value(evaluate(
                &book(),
                "SUM(IF({TRUE,FALSE,TRUE},{1,2},9))",
                arrays
            )),
            CellValue::Error(CellError::NA)
        );
    }
}
#[test]
fn selected_references_skip_unneeded_members_and_operational_errors_are_not_caught() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target("D2").address, "COUNTIF(A1:A3,E1:E2)")
            .unwrap();
        assert_eq!(
            value(evaluate(
                &book,
                "SUM(IF({TRUE;FALSE;TRUE},A1:A3,B1:B3))",
                arrays
            )),
            CellValue::Number(6.)
        );
        assert_eq!(
            value(evaluate(&book, "SUM(IF({TRUE,TRUE},2,A2))", arrays)),
            CellValue::Number(4.)
        );
        assert!(matches!(
            evaluate(&book, "IFERROR(SUM(IF({TRUE,FALSE},D2,2)),99)", arrays),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn selected_volatile_branch_is_evaluated_once() {
    let result = evaluate(
        &book(),
        "SUM(IF({TRUE,TRUE,TRUE},RAND(),RAND()))",
        ArrayResultPolicy::ScalarIntersection,
    );
    assert_eq!(result.usage().random_accesses.len(), 1);
    assert_eq!(result.usage().random_accesses.first().unwrap().draws, 1);
    assert!(matches!(value(result), CellValue::Number(n) if (0.0..3.0).contains(&n)));
    let both = evaluate(
        &book(),
        "SUM(IF({TRUE,FALSE,TRUE},RAND(),RAND()))",
        ArrayResultPolicy::ScalarIntersection,
    );
    assert_eq!(both.usage().random_accesses.first().unwrap().draws, 2);
}
#[test]
fn condition_dependency_changes_selected_members_after_snapshot_restore() {
    let mut book = book();
    let sheet = book.sheet_mut(0).unwrap();
    for (cell, value) in [("C1", true), ("C2", false), ("C3", true)] {
        sheet.set_value(target(cell).address, value).unwrap();
    }
    sheet
        .set_formula(target("E1").address, "SUMPRODUCT(IF(C1:C3,A1:A3,10))")
        .unwrap();
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(OverlayPolicy {
                arrays,
                builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
                ..Default::default()
            })
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("E1"))
                .unwrap(),
            CellValue::Number(14.)
        );
        let (restored_book, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
        let mut overlay = state.attach(&restored_book).unwrap();
        overlay.set_value(target("C1"), false).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("E1"))
                .unwrap(),
            CellValue::Number(23.)
        );
        // A2 was never selected; changing it cannot change the numeric result.
        overlay.set_value(target("A2"), 100.).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("E1"))
                .unwrap(),
            CellValue::Number(23.)
        );
    }
}
#[test]
fn pinned_if_retains_scalar_condition_selection() {
    let result = run_formula_with_configuration(
        &book(),
        0,
        target("Z100").address,
        "SUM(IF({TRUE,FALSE,TRUE},2,{30,5,70}))",
        &RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi)
            .with_array_result_policy(ArrayResultPolicy::ScalarIntersection),
        EvaluationBudgets::default(),
    );
    assert_eq!(value(result), CellValue::Number(2.));
}

#[test]
fn policy_changes_invalidate_cached_if_results_in_both_directions() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E1").address,
            "SUM(IF({TRUE,FALSE,TRUE},2,{30,5,70}))",
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    for (builtin, expected) in [
        (BuiltinSemanticsPolicy::PinnedPoi, 2.),
        (BuiltinSemanticsPolicy::ExcelDocumentation, 9.),
        (BuiltinSemanticsPolicy::PinnedPoi, 2.),
    ] {
        overlay
            .set_policy(OverlayPolicy {
                builtin,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("E1"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
}
#[test]
fn singleton_conditions_keep_reference_and_inline_blank_identity() {
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for condition in ["TRUE", "{TRUE}"] {
            for (formula, expected) in [
                (
                    format!("ISREF(IF({condition},X1:X3,0))"),
                    CellValue::Boolean(condition == "TRUE"),
                ),
                (
                    format!("AND(ISBLANK(IF({condition},D1,0)))"),
                    CellValue::Boolean(true),
                ),
                (
                    format!("COUNT(IF({condition},D1,0))"),
                    CellValue::Number(0.),
                ),
            ] {
                assert_eq!(
                    value(evaluate(&book(), &formula, arrays)),
                    expected,
                    "{arrays:?} {formula}"
                );
            }
        }
    }
}

#[test]
fn condition_dimensions_bound_dynamic_spills_before_anchor_reads() {
    for (formula, checks) in [
        ("IF({TRUE;FALSE},1,2)", vec![("E2", 2.), ("E1", 1.)]),
        (
            "IF({TRUE;FALSE},{1,2,3},{10,20,30})",
            vec![("G2", 30.), ("F1", 2.), ("E1", 1.)],
        ),
    ] {
        let mut book = book();
        book.sheet_mut(0)
            .unwrap()
            .set_formula(target("E1").address, formula)
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
        for (cell, expected) in checks {
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
