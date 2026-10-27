//! First-true array selection follows the existing lazy argument contract.
use poirot::eval::overlay::Overlay;
use poirot::eval::*;
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
    RunConfiguration::default().with_array_result_policy(arrays)
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
        ("K1", "COUNTIF(A1:A3,)"),
    ] {
        sheet.set_formula(target(cell).address, formula).unwrap();
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
fn condition_edits_invalidate_retained_results_after_restore() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E1").address,
            "SUM(IFS({TRUE,FALSE,TRUE},{3,100,7},TRUE,5))",
        )
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E3").address,
            "SUMPRODUCT(IFS(C1:C3,A1:A3,TRUE,B1:B3))",
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
}
#[test]
fn condition_and_selected_result_dimensions_bound_child_first_spills() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("E5").address,
            "IFS({TRUE;FALSE},{1,2,3},TRUE,{10,20,30})",
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            arrays: ArrayResultPolicy::DynamicSpill,

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

#[test]
fn computed_selector_probes_execute_once_with_selected_member_reads() {
    use poirot::eval::observe::OperationKind;
    use poirot::eval::observe::{BoundedExecutionTrace, RetainedEvent, RetentionLimits};
    /// Record every read, including repeated reads of the same physical cell.
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
    let configuration = config(ArrayResultPolicy::ScalarIntersection);
    // The service contract is one execution per demanded probe. RAND is multiplied
    // by zero, so its draw count observes evaluation without changing the mask.
    for formula in [
        "SUMPRODUCT(IFS((A1:A3>2)+RAND()*0,B1:B3,TRUE,A1:A3))",
        "SUMPRODUCT(SWITCH((A1:A3>2)+RAND()*0,1,B1:B3,0,A1:A3))",
    ] {
        for observed in [false, true] {
            let prepared = compiler.compile_formula(formula, &policy);
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
                ["A1", "A2", "A3", "A1", "B2", "B3"].map(|cell| target(cell).address),
                "{formula} observed={observed}"
            );
            assert_eq!(
                report
                    .outcome
                    .usage()
                    .random_accesses
                    .first()
                    .unwrap()
                    .draws,
                1,
                "{formula} observed={observed}"
            );
            assert_eq!(value(report.outcome), CellValue::Number(303.));
            if observed {
                let first_probe_observations = trace
                    .events()
                    .iter()
                    .filter(|event| {
                        matches!(event,
                    RetainedEvent::Begin(operation) if matches!(operation.kind,
                        OperationKind::Argument { ordinal: 0, .. }))
                    })
                    .count();
                assert_eq!(first_probe_observations, 1);
            }
        }
    }
}
