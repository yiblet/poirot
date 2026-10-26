//! Positional IF selection shares rectangular traversal without changing pinned scalar semantics.
use poirot::eval::{
    overlay::{EvaluationContext, EvaluationPolicy, Overlay},
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
            .set_formula(target("D2").address, "COUNTIF(A1:A3,)")
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
            .with_policy(EvaluationPolicy {
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
fn scalar_if_forces_only_the_selected_branch() {
    for builtin in [
        BuiltinSemanticsPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation,
    ] {
        let config = RunConfiguration::default().with_builtin_policy(builtin);
        let outcome = run_formula_with_configuration(
            &book(),
            0,
            target("Z100").address,
            "IF(TRUE,RAND(),RAND())",
            &config,
            EvaluationBudgets::default(),
        );
        assert_eq!(outcome.usage().random_accesses[0].draws, 1);
        assert!(matches!(value(outcome), CellValue::Number(n) if (0.0..1.0).contains(&n)));
        assert_eq!(
            value(run_formula_with_configuration(
                &book(),
                0,
                target("Z100").address,
                "IF(FALSE,1/0,7)",
                &config,
                EvaluationBudgets::default(),
            )),
            CellValue::Number(7.)
        );
    }
}

#[test]
fn selected_if_work_limits_remain_operational() {
    for formula in [
        "IFERROR(IF(TRUE,1,2),99)",
        "IFERROR(SUM(IF({TRUE;FALSE},{1,2,3},9)),99)",
    ] {
        assert!(matches!(
            run_formula_with_configuration(
                &book(),
                0,
                target("Z100").address,
                formula,
                &config(ArrayResultPolicy::ScalarIntersection),
                EvaluationBudgets {
                    run_work: 5,
                    ..Default::default()
                },
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
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
            .set_policy(EvaluationPolicy {
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
            .with_policy(EvaluationPolicy {
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

#[test]
fn deferred_if_omission_and_condition_errors_do_not_prepare_branches() {
    // IF's documented selected-result rule, with existing omitted-versus-absent
    // and retained-array behavior corroborated by this suite's independent probes.
    for arrays in [
        ArrayResultPolicy::ScalarIntersection,
        ArrayResultPolicy::DynamicSpill,
    ] {
        for (formula, expected) in [
            ("ISBLANK(IF(TRUE,,RAND()))", CellValue::Boolean(true)),
            ("IF(FALSE,RAND())", CellValue::Boolean(false)),
            ("SUM(IF({FALSE,FALSE},RAND()))", CellValue::Number(0.)),
            (
                "SUM(IF({#REF!,#REF!},RAND(),RAND()))",
                CellValue::Error(CellError::Ref),
            ),
            (
                "SUM(IF({TRUE,#REF!,FALSE},1,2))",
                CellValue::Error(CellError::Ref),
            ),
            ("IF(#REF!,RAND(),RAND())", CellValue::Error(CellError::Ref)),
        ] {
            let result = evaluate(&book(), formula, arrays);
            assert!(result.usage().random_accesses.is_empty(), "{formula}");
            assert_eq!(value(result), expected, "{arrays:?} {formula}");
        }
    }
}

#[test]
fn deferred_if_observation_keeps_branch_order_and_member_dependencies() {
    use poirot::eval::execution_observation::OperationKind;
    use poirot::eval::execution_retention::{
        BoundedExecutionTrace, RetainedEvent, RetentionLimits,
    };
    /// Retain exact dependency reads to compare observation modes and masks.
    #[derive(Default)]
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let book = book();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&book, &policy);
    // The first selected coordinate is false; preparation must still visit true first.
    let prepared = compiler.compile_formula("SUM(IF({FALSE;TRUE;FALSE},B1:B3,A1:A3))", &policy);
    let configuration = config(ArrayResultPolicy::ScalarIntersection);
    let mut baseline = None;
    for observed in [false, true] {
        let mut reads = Reads::default();
        let mut trace = BoundedExecutionTrace::new(RetentionLimits {
            events: 1000,
            scalar_values: 1000,
            text_bytes: 10000,
        });
        let mut run = EvaluationRun::new_with_configuration(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            &configuration,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        if observed {
            run = run.with_execution_observer(&mut trace);
        }
        let result = run.evaluate_formula(target("Z100"), &prepared);
        let report = run.finish(result);
        assert!(matches!(
            &report.outcome,
            EvaluationOutcome::Complete {
                value: CellValue::Number(6.),
                ..
            }
        ));
        let members: Vec<_> = reads
            .0
            .iter()
            .filter_map(|event| match event.read {
                DependencyRead::Cell(target) => Some(target.address),
                _ => None,
            })
            .collect();
        assert_eq!(
            members,
            [
                target("A1").address,
                target("B2").address,
                target("A3").address
            ]
        );
        if observed {
            let arguments: Vec<_> = trace
                .events()
                .iter()
                .filter_map(|event| match event {
                    RetainedEvent::Begin(operation) => match operation.kind {
                        OperationKind::Argument { ordinal, .. } => Some(ordinal),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            // Each selected IF ordinal is observed once; SUM consumes its input directly.
            assert_eq!(arguments, [0, 1, 2]);
            let (outcome, execution, prior_reads) = baseline.take().unwrap();
            assert_eq!(report.outcome, outcome);
            assert_eq!(report.execution, execution);
            assert_eq!(reads.0, prior_reads);
        } else {
            baseline = Some((report.outcome, report.execution, reads.0));
        }
    }
}
