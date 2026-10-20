use poirot_eval::{execution_observation::*, execution_retention::*, *};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
const AT: CellAddress = CellAddress { row: 0, col: 0 };
fn limits() -> RetentionLimits {
    RetentionLimits {
        events: 1000,
        scalar_values: 1000,
        text_bytes: 10000,
    }
}
fn evaluate(
    formula: &str,
    trace: Option<&mut BoundedExecutionTrace>,
    budgets: EvaluationBudgets,
) -> RunReport<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let prepared = compiler.compile_formula(formula, &policy);
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        budgets,
    )
    .unwrap();
    let mut run = match trace {
        Some(trace) => run.with_execution_observer(trace),
        None => run,
    };
    let result = run.evaluate_formula(
        EvaluationTarget {
            sheet: 0,
            address: AT,
        },
        &prepared,
    );
    run.finish(result)
}
fn value(report: RunReport<CellValue>) -> CellValue {
    match report.outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
#[test]
fn actual_lookup_na_and_nested_selected_fallback_ids() {
    let mut trace = BoundedExecutionTrace::new(limits());
    assert_eq!(
        value(evaluate(
            "IFERROR(IFNA(VLOOKUP(9,{1,2},2,FALSE),7),RAND())",
            Some(&mut trace),
            EvaluationBudgets::default()
        )),
        CellValue::Number(7.)
    );
    let events = trace.events();
    let operations: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RetainedEvent::Begin(op) => Some(*op),
            _ => None,
        })
        .collect();
    assert!(operations.windows(2).all(|w| w[0].id.0 < w[1].id.0));
    for op in &operations {
        if let Some(parent) = op.parent {
            assert!(
                operations
                    .iter()
                    .any(|p| p.id == parent && p.id.0 < op.id.0)
            );
        }
    }
    let decisions: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            RetainedEvent::MaskDecision {
                handler,
                source_argument,
                caught,
                selected_fallback_operation,
                ..
            } => Some((
                *handler,
                *source_argument,
                *caught,
                *selected_fallback_operation,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(decisions.len(), 2);
    let caught = decisions
        .iter()
        .find(|d| d.2 == Some(CellError::NA))
        .unwrap();
    let fallback = operations
        .iter()
        .find(|op| Some(op.id) == caught.3)
        .unwrap();
    assert_eq!(fallback.parent, Some(caught.0));
    assert!(matches!(
        fallback.kind,
        OperationKind::Argument { ordinal: 1, .. }
    ));
    assert!(events.iter().any(|e| matches!(e, RetainedEvent::Complete { operation, result: Ok(RetainedValue::Scalar(CellValue::Error(CellError::NA))) } if operation.id == caught.1)));
    assert!(!operations.iter().any(|op| matches!(
        op.kind,
        OperationKind::Expression {
            handler: Some(registry::FunctionId::Math(registry::MathFunction::Rand))
        }
    )));
}
#[test]
fn ifna_preserves_value_and_never_runs_fallback() {
    let mut trace = BoundedExecutionTrace::new(limits());
    assert_eq!(
        value(evaluate(
            "IFNA(#VALUE!,RAND())",
            Some(&mut trace),
            EvaluationBudgets::default()
        )),
        CellValue::Error(CellError::Value)
    );
    assert!(trace.events().iter().any(|e| matches!(
        e,
        RetainedEvent::MaskDecision {
            caught: None,
            selected_fallback_operation: None,
            ..
        }
    )));
    assert!(!trace.events().iter().any(|e| matches!(
        e,
        RetainedEvent::Begin(Operation {
            kind: OperationKind::Argument { ordinal: 1, .. },
            ..
        })
    )));
}
#[test]
fn event_value_and_text_caps_leave_partial_without_masking_operational_failure() {
    for limits in [
        RetentionLimits {
            events: 1,
            ..limits()
        },
        RetentionLimits {
            scalar_values: 0,
            ..limits()
        },
        RetentionLimits {
            text_bytes: 0,
            ..limits()
        },
    ] {
        let mut trace = BoundedExecutionTrace::new(limits);
        let report = evaluate(
            "IFERROR(\"payload\",7)",
            Some(&mut trace),
            EvaluationBudgets::default(),
        );
        assert!(matches!(
            report.outcome,
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::ResourceLimit { .. },
                ..
            }
        ));
        assert!(trace.is_partial());
        assert!(!trace.events().iter().any(|e| matches!(
            e,
            RetainedEvent::MaskDecision {
                caught: Some(_),
                ..
            }
        )));
    }
    let mut trace = BoundedExecutionTrace::new(limits());
    let report = evaluate(
        "IFERROR(REPT(\"x\",100),7)",
        Some(&mut trace),
        EvaluationBudgets {
            text_bytes: 30,
            ..EvaluationBudgets::default()
        },
    );
    assert!(matches!(
        report.outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    assert!(trace.is_partial());
    assert!(!trace.events().iter().any(|e| matches!(
        e,
        RetainedEvent::MaskDecision {
            caught: Some(_),
            ..
        }
    )));
}
#[test]
fn observation_preserves_output_usage_rng_and_lazy_execution() {
    for formula in [
        "IF(TRUE,RAND(),NOW())",
        "IFERROR(1,RAND())",
        "IFERROR(IFNA(#N/A,RAND()),NOW())",
    ] {
        let without = evaluate(formula, None, EvaluationBudgets::default());
        let mut trace = BoundedExecutionTrace::new(limits());
        let with = evaluate(formula, Some(&mut trace), EvaluationBudgets::default());
        assert_eq!(without.execution, with.execution);
        match (without.outcome, with.outcome) {
            (
                EvaluationOutcome::Complete {
                    value: a,
                    usage: au,
                },
                EvaluationOutcome::Complete {
                    value: b,
                    usage: bu,
                },
            ) => {
                assert_eq!(a, b);
                assert_eq!(au, bu);
            }
            other => panic!("{other:?}"),
        }
    }
}
#[test]
fn callback_failure_is_operational_and_trace_frame_is_cleaned_for_next_probe() {
    /// Refuses a controlled callback to test operational failure and frame cleanup.
    struct Observer {
        begins: Vec<Operation>,
        fail_once: bool,
    }
    impl ExecutionObserver for Observer {
        fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationFailure> {
            if let ExecutionEvent::Begin(operation) = event {
                self.begins.push(operation);
            }
            if matches!(
                event,
                ExecutionEvent::MaskDecision {
                    caught: Some(_),
                    ..
                }
            ) && self.fail_once
            {
                self.fail_once = false;
                return Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: None,
                    detail: "observer fidelity refusal".into(),
                });
            }
            Ok(())
        }
    }
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let policy = EvaluationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let prepared = compiler.compile_formula("IFERROR(#N/A,RAND())", &policy);
    let second = compiler.compile_formula("7", &policy);
    let mut observer = Observer {
        begins: Vec::new(),
        fail_once: true,
    };
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_execution_observer(&mut observer);
    let target = EvaluationTarget {
        sheet: 0,
        address: AT,
    };
    assert!(matches!(
        run.evaluate_formula(target, &prepared),
        Err(EvaluationFailure::UnsupportedCapability { .. })
    ));
    assert_eq!(
        run.evaluate_formula(target, &second).unwrap(),
        CellValue::Number(7.)
    );
    let report = run.finish(Ok(()));
    assert!(report.outcome.usage().random_accesses.is_empty());
    assert_eq!(observer.begins.last().unwrap().parent, None);
}
#[test]
fn read_events_match_with_observation_enabled_and_disabled() {
    #[derive(Default)]
    /// Collects reads for comparison with execution observation enabled or disabled.
    struct Reads(Vec<ReadEvent>);
    impl DependencyReadSink for Reads {
        fn read(&mut self, event: ReadEvent) {
            self.0.push(event);
        }
    }
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(AT, "IF(TRUE,B1,C1)")
        .unwrap();
    let policy = EvaluationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let target = EvaluationTarget {
        sheet: 0,
        address: AT,
    };
    let mut without_reads = Reads::default();
    let mut without = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut without_reads);
    let a = without.evaluate_cell(target);
    let a = without.finish(a);
    let mut with_reads = Reads::default();
    let mut trace = BoundedExecutionTrace::new(limits());
    let mut with = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        EvaluationInputs::default(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut with_reads)
    .with_execution_observer(&mut trace);
    let b = with.evaluate_cell(target);
    let b = with.finish(b);
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.execution, b.execution);
    assert_eq!(without_reads.0, with_reads.0);
    assert!(
        !with_reads
            .0
            .iter()
            .any(|e| matches!(e.read, DependencyRead::Cell(t) if t.address.col == 2))
    );
}
