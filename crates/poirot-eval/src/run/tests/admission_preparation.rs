//! Tests of the admission_preparation contract.
use super::*;

#[test]
fn prepared_cell_execution_borrows_plan_formula_during_dependency_reads() {
    /// Observes compiled formula ownership while recursive dependency reads execute.
    struct FormulaOwnership<'a> {
        formula: &'a Arc<PreparedFormula>,
        owners: usize,
        dependency_reads: usize,
    }
    impl CellObserver for FormulaOwnership<'_> {
        fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
            if completion.target == target(A1) {
                assert_eq!(completion.consumer, Some(target(B1)));
                assert_eq!(Arc::strong_count(self.formula), self.owners);
                self.dependency_reads += 1;
            }
            Ok(())
        }
    }

    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(A1, 2.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "A1+1")
        .unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let formula = plan.formula(target(B1)).unwrap();
    let mut observer = FormulaOwnership {
        formula,
        owners: Arc::strong_count(formula),
        dependency_reads: 0,
    };
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_observer(&mut observer);
    assert_eq!(
        run.evaluate_cell(target(B1)).unwrap(),
        CellValue::Number(3.)
    );
    assert_eq!(
        run.evaluate_cell(target(B1)).unwrap(),
        CellValue::Number(3.)
    );
    assert_eq!(run.usage().formula_executions, 1);
    assert_eq!(run.usage().compiled_lookups, 1);
    assert_eq!(run.usage().session_memo_hits, 1);
    drop(run);
    assert_eq!(observer.dependency_reads, 1);
}

#[test]
fn repeated_runs_reuse_preparation_but_read_current_values_and_signed_zero() {
    let mut workbook = workbook();
    workbook.sheet_mut(0).unwrap().set_value(A1, 0.).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "A1")
        .unwrap();
    let mut compiler = FormulaCompiler::new();
    let policy = EvaluationPolicy::default();
    let plan = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(compiler.usage().parse_attempts, 1);
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    let first = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
    assert_eq!(first.execution.formula_executions, 1);
    assert_eq!(first.execution.compiled_lookups, 1);
    let first = value(first.outcome)[0].value.clone();
    workbook.sheet_mut(0).unwrap().get_mut(A1).unwrap().value = CellValue::Number(-0.);
    assert!(plan.matches(&workbook, &policy));
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    let second = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
    assert_eq!(second.execution.formula_executions, 1);
    assert_eq!(second.execution.session_memo_hits, 0);
    let second = value(second.outcome)[0].value.clone();
    assert!(!same_result_identity(&first, &second));
    assert!(same_result_identity(&second, &CellValue::Number(-0.)));
    let _ = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(
        compiler.usage(),
        &CompilerUsage {
            parse_attempts: 1,
            bind_attempts: 1,
            reused_formulas: 1
        }
    );
}
#[test]
fn exact_topology_and_policy_checks_detect_direct_mutation() {
    let mut original = workbook();
    original.add_sheet("Other").unwrap();
    original
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "Rate")
        .unwrap();
    original.define_name("Rate", "3", None).unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&original, &policy);
    let check = |workbook: &Workbook| {
        assert!(!plan.matches(workbook, &policy));
        assert!(matches!(
            WorkbookView::new(workbook, &plan, &policy),
            Err(EvaluationFailure::StalePlan)
        ));
    };
    let mut changed = original.clone();
    changed.sheet_mut(0).unwrap().get_mut(A1).unwrap().formula = Some("4".into());
    check(&changed);
    let _ = compiler.compile_workbook(&changed, &policy);
    assert_eq!(compiler.usage().parse_attempts, 3); // Sources: "Rate", name body "3", replacement "4".
    changed = original.clone();
    changed.sheet_mut(0).unwrap().get_mut(A1).unwrap().formula = None;
    check(&changed);
    changed = original.clone();
    changed.sheet_mut(0).unwrap().set_formula(B1, "2").unwrap();
    check(&changed);
    changed = original.clone();
    changed.named_ranges[0].formula = "4".into();
    check(&changed);
    changed = original.clone();
    changed.named_ranges[0].sheet_scope = Some(0);
    check(&changed);
    changed = original.clone();
    changed.rename_sheet(0, "Renamed").unwrap();
    check(&changed);
    changed = original.clone();
    changed.sheets_mut().swap(0, 1);
    check(&changed);
    let mut other_policy = policy.clone();
    other_policy.parse.max_expressions = 8;
    assert!(!plan.matches(&original, &other_policy));
    other_policy = policy.clone();
    other_policy.registry = RegistryIdentity {
        builtin_contract_version: RegistryIdentity::default().builtin_contract_version + 1,
        ..RegistryIdentity::default()
    };
    assert!(!plan.matches(&original, &other_policy));
}
#[test]
fn standalone_policy_mismatch_is_operational_and_parse_failure_keeps_text_budget() {
    let workbook = workbook();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let changed = EvaluationPolicy {
        registry: RegistryIdentity {
            builtin_contract_version: RegistryIdentity::default().builtin_contract_version + 1,
            ..RegistryIdentity::default()
        },
        ..policy.clone()
    };
    let prepared = compiler.compile_formula("1", &changed);
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap();
    assert!(matches!(
        run.evaluate_probes(&[EvaluationProbe::Formula {
            target: target(A1),
            formula: &prepared
        }])
        .outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::StalePlan,
            ..
        }
    ));
    let outcome = crate::run_formula(
        &workbook,
        0,
        A1,
        "12345+",
        context(),
        EvaluationBudgets {
            text_bytes: 3,
            ..EvaluationBudgets::default()
        },
    );
    assert!(matches!(
        outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
#[test]
fn source_deduplication_and_context_changes_do_not_reprepare() {
    let mut workbook = workbook();
    workbook.define_name("Constant", "3", None).unwrap();
    workbook.sheet_mut(0).unwrap().set_formula(A1, "3").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "NOW()")
        .unwrap();
    let mut compiler = FormulaCompiler::new();
    let policy = EvaluationPolicy::default();
    let plan = compiler.compile_workbook(&workbook, &policy);
    assert_eq!(
        compiler.usage(),
        &CompilerUsage {
            parse_attempts: 2,
            bind_attempts: 2,
            reused_formulas: 1
        }
    );
    let mut outputs = Vec::new();
    for time in [UNIX_EPOCH, UNIX_EPOCH + Duration::from_secs(86400)] {
        let mut context = context();
        context.as_of_time = time;
        let mut reads = Reads::default();
        let run = EvaluationRun::new(
            WorkbookView::new(&workbook, &plan, &policy).unwrap(),
            context,
            EvaluationBudgets::default(),
        )
        .unwrap()
        .with_dependency_sink(&mut reads);
        let report = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
        assert_eq!(report.execution.formula_executions, 1);
        outputs.push(value(report.outcome)[0].value.clone());
        assert!(
            reads
                .0
                .iter()
                .any(|e| e.read == DependencyRead::ContextField(ContextRead::AsOf))
        );
    }
    assert!(!same_result_identity(&outputs[0], &outputs[1]));
    assert_eq!(compiler.usage().parse_attempts, 2);
}
