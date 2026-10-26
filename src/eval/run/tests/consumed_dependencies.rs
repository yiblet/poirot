//! Tests of the consumed_dependencies contract.
use super::*;

#[test]
fn shared_probes_report_dependency_edges_even_when_memoized() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(A1, 3.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "A1+1")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(C1, "A1+2")
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    let report = run.evaluate_probes(&[
        EvaluationProbe::Cell(target(B1)),
        EvaluationProbe::Cell(target(C1)),
    ]);
    assert_eq!(report.execution.formula_executions, 2);
    assert_eq!(report.execution.session_memo_hits, 1);
    assert_eq!(
        value(report.outcome)
            .iter()
            .map(|v| v.value.clone())
            .collect::<Vec<_>>(),
        vec![CellValue::Number(4.), CellValue::Number(5.)]
    );
    for consumer in [B1, C1] {
        assert!(reads.0.contains(&ReadEvent {
            consumer: Some(target(consumer)),
            read: DependencyRead::Cell(target(A1))
        }));
    }
}
#[test]
fn skipped_opaque_and_invalid_name_bodies_remain_deferred() {
    let mut workbook = workbook();
    workbook.define_name("Broken", "1+", None).unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let lazy = compiler.compile_formula("IF(FALSE,UNKNOWN(Broken),1)", &policy);
    let invalid = compiler.compile_formula("IF(FALSE,Broken,2)", &policy);
    let selected = compiler.compile_formula("Broken", &policy);
    if let PreparedFormula::Ready(formula) = &*lazy {
        assert_eq!(
            formula.potential_dependencies().opaque_functions,
            vec!["UNKNOWN"]
        );
    } else {
        panic!("lazy formula must prepare");
    }
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    let result = value(
        run.evaluate_probes(&[
            EvaluationProbe::Formula {
                target: target(A1),
                formula: &lazy,
            },
            EvaluationProbe::Formula {
                target: target(B1),
                formula: &invalid,
            },
        ])
        .outcome,
    );
    assert_eq!(result[0].value, CellValue::Number(1.));
    assert_eq!(result[1].value, CellValue::Number(2.));
    assert!(!reads.0.iter().any(|e| matches!(
        e.read,
        DependencyRead::OpaqueProvider(_) | DependencyRead::Name { .. }
    )));
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Formula {
                target: target(A1),
                formula: &selected
            }])
            .outcome
        )[0]
        .value,
        CellValue::Error(CellError::Value)
    );
    let before = compiler.usage().parse_attempts;
    let _ = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(compiler.usage().parse_attempts, before);
}
#[test]
fn names_are_reused_and_namespace_selection_is_read_at_execution() {
    let mut workbook = workbook();
    workbook.define_name("Rate", "A1", None).unwrap();
    workbook.define_name("Rate", "A1+1", Some(0)).unwrap();
    workbook.sheet_mut(0).unwrap().set_value(A1, 6.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "Rate")
        .unwrap();
    let mut compiler = FormulaCompiler::new();
    let policy = PreparationPolicy::default();
    let plan = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(compiler.usage().parse_attempts, 3);
    for _ in 0..2 {
        let mut reads = Reads::default();
        let run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            context(),
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let report = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
        assert_eq!(report.execution.named_formula_executions, 1);
        assert_eq!(report.execution.compiled_lookups, 2);
        assert_eq!(value(report.outcome)[0].value, CellValue::Number(7.));
        assert!(reads.0.iter().any(|e| matches!(
            &e.read,
            DependencyRead::Name {
                selected: Some(1),
                ..
            }
        )));
    }
    assert_eq!(compiler.usage().parse_attempts, 3);
    workbook.named_ranges.swap(0, 1);
    assert!(!plan.matches(&workbook, &policy));
}
#[test]
fn sparse_membership_covers_future_insertions_and_actual_cells() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(A1, 2.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "SUM(A:A)")
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))])
                .outcome
        )[0]
        .value,
        CellValue::Number(2.)
    );
    assert!(
        reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::SparseMembership { sheet: 0, .. }))
    );
    let a2 = CellAddress { row: 1, col: 0 };
    assert!(
        !reads
            .0
            .iter()
            .any(|e| e.read == DependencyRead::Cell(target(a2)))
    );
    workbook.sheet_mut(0).unwrap().set_value(a2, 5.).unwrap();
    assert!(plan.matches(&workbook, &policy));
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))])
                .outcome
        )[0]
        .value,
        CellValue::Number(7.)
    );
}
#[test]
fn selected_unknown_records_opaque_consumption_and_abort_keeps_values_unpublished() {
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(A1, "1", CellValue::Number(99.))
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(B1, "2+3", CellValue::Number(88.))
        .unwrap();
    let before = workbook.clone();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let unknown = compiler.compile_formula("UNKNOWN()", &policy);
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads);
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Formula {
                target: target(C1),
                formula: &unknown
            }])
            .outcome
        )[0]
        .value,
        CellValue::Error(CellError::Name)
    );
    assert!(
        reads
            .0
            .iter()
            .any(|e| e.read == DependencyRead::OpaqueProvider("UNKNOWN".into()))
    );
    let budgets = EvaluationBudgets {
        run_work: 5,
        ..EvaluationBudgets::default()
    };
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        budgets,
    )
    .unwrap();
    let report = run.evaluate_probes(&[
        EvaluationProbe::Cell(target(A1)),
        EvaluationProbe::Cell(target(B1)),
    ]);
    assert!(matches!(
        report.outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert_eq!(workbook, before);
    assert!(!same_result_identity(
        &CellValue::Number(f64::from_bits(0x7ff8000000000001)),
        &CellValue::Number(f64::from_bits(0x7ff8000000000002))
    ));
}

#[test]
fn indirect_parses_current_operands_at_runtime_and_honors_parse_policy() {
    let mut workbook = workbook();
    let a2 = CellAddress { row: 1, col: 0 };
    let a3 = CellAddress { row: 2, col: 0 };
    workbook.sheet_mut(0).unwrap().set_value(A1, "A2").unwrap();
    workbook.sheet_mut(0).unwrap().set_value(a2, 3.).unwrap();
    workbook.sheet_mut(0).unwrap().set_value(a3, 7.).unwrap();
    for at in [B1, C1] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_formula(at, "INDIRECT(A1)")
            .unwrap();
    }
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(compiler.usage().parse_attempts, 1);
    for (source, expected) in [("A2", 3.), ("A3", 7.)] {
        workbook.sheet_mut(0).unwrap().get_mut(A1).unwrap().value = CellValue::Text(source.into());
        let run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            context(),
            EvaluationBudgets::default(),
        )
        .unwrap();
        let report = run.evaluate_probes(&[
            EvaluationProbe::Cell(target(B1)),
            EvaluationProbe::Cell(target(C1)),
        ]);
        assert_eq!(report.execution.dynamic_parse_attempts, 2);
        assert_eq!(report.execution.formula_executions, 2);
        assert_eq!(
            value(report.outcome)
                .iter()
                .map(|v| v.value.clone())
                .collect::<Vec<_>>(),
            vec![CellValue::Number(expected); 2]
        );
    }
    let restricted = PreparationPolicy {
        parse: poirot_parsing::formula::FormulaParsePolicy {
            max_rows: 1,
            ..policy.parse
        },
        ..policy.clone()
    };
    let plan = compiler.compile_workbook(&workbook, &restricted);
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &restricted).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    let report = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
    // Only the producer whose possible rectangle covers this demand executes.
    assert_eq!(report.execution.dynamic_parse_attempts, 1);
    assert_eq!(
        value(report.outcome)[0].value,
        CellValue::Error(CellError::Ref)
    );
}
