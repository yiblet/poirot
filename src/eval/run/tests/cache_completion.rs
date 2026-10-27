//! Tests of the cache_completion contract.
use super::super::evaluation_probe::EvaluationProbe;
use super::super::evaluation_run::EvaluationRun;
use super::super::workbook_view::WorkbookView;
use super::support::{A1, B1, C1, Reads, context, target, value, workbook};
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvaluationBudgets, EvaluationError, EvaluationOutcome};
use crate::eval::ports::{CachedCellReader, CellCompletion, CellObserver, CellProvenance};
use crate::eval::ports::{DependencyRead, ReadEvent};
use crate::model::CellValue;
use std::sync::Arc;

#[test]
fn external_cache_port_preserves_consumed_edges_and_operational_failures() {
    /// Test cache recording queries and optionally rejecting reads operationally.
    struct Cached {
        queries: Vec<EvaluationTarget>,
        failure: bool,
    }
    impl CachedCellReader for Cached {
        fn cached_cell(
            &mut self,
            target: EvaluationTarget,
        ) -> Result<Option<Arc<CellValue>>, EvaluationError> {
            self.queries.push(target);
            if self.failure {
                return Err(EvaluationError::UnsupportedCapability {
                    capability: crate::eval::limits::CapabilityId::ExternalData,
                    target: Some(target),
                    reason: crate::eval::limits::EvaluationReasonError::ExternalSourceUnavailable,
                });
            }
            Ok((target.address == A1).then(|| Arc::new(CellValue::Number(42.))))
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "40+2")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "A1+1")
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut reader = Cached {
        queries: Vec::new(),
        failure: false,
    };
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_reader(&mut reader)
    .with_dependency_sink(&mut reads);
    let report = run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]);
    assert_eq!(report.execution.formula_executions, 1);
    assert_eq!(report.execution.external_cache_hits, 1);
    assert_eq!(value(report.outcome)[0].value, CellValue::Number(43.));
    assert_eq!(reader.queries, vec![target(B1), target(A1)]);
    assert!(reads.0.contains(&ReadEvent {
        consumer: Some(target(B1)),
        read: DependencyRead::Cell(target(A1))
    }));
    reader.failure = true;
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_reader(&mut reader);
    assert!(matches!(
        run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))])
            .outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::UnsupportedCapability { .. },
            ..
        }
    ));
}

#[test]
fn completed_cell_observer_sees_recursive_results_and_all_provenances() {
    /// Records completed values and their actual reuse provenance.
    #[derive(Default)]
    struct Observer(Vec<(EvaluationTarget, CellProvenance, CellValue)>);
    impl CellObserver for Observer {
        fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationError> {
            self.0.push((
                completion.target,
                completion.provenance,
                completion.result.unwrap().clone(),
            ));
            Ok(())
        }
    }
    /// Supplies a fixed external result for one selected cell.
    struct Cache;
    impl CachedCellReader for Cache {
        fn cached_cell(
            &mut self,
            target: EvaluationTarget,
        ) -> Result<Option<Arc<CellValue>>, EvaluationError> {
            Ok((target.address == C1).then(|| Arc::new(CellValue::Number(44.))))
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "40+2")
        .unwrap();
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
    let mut observer = Observer::default();
    let mut cache = Cache;
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_reader(&mut cache)
    .with_cell_observer(&mut observer);
    let report = run.evaluate_probes(&[
        EvaluationProbe::Cell(target(B1)),
        EvaluationProbe::Cell(target(A1)),
        EvaluationProbe::Cell(target(C1)),
    ]);
    assert_eq!(value(report.outcome).len(), 3);
    assert_eq!(
        observer.0,
        vec![
            (
                target(A1),
                CellProvenance::Evaluated,
                CellValue::Number(42.)
            ),
            (
                target(B1),
                CellProvenance::Evaluated,
                CellValue::Number(43.)
            ),
            (
                target(A1),
                CellProvenance::SessionMemo,
                CellValue::Number(42.)
            ),
            (
                target(C1),
                CellProvenance::ExternalCache,
                CellValue::Number(44.)
            ),
        ]
    );
}

#[test]
fn bounded_observer_failure_aborts_and_prevents_transaction_commit() {
    /// Stages one completion before refusing further retention to test atomic aborts.
    struct Transaction {
        staged: Vec<CellValue>,
        aborts: Vec<EvaluationTarget>,
    }
    impl CellObserver for Transaction {
        fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationError> {
            match completion.result {
                Err(_) => {
                    self.aborts.push(completion.target);
                    Ok(())
                }
                Ok(value) if self.staged.is_empty() => {
                    self.staged.push(value.clone());
                    Ok(())
                }
                Ok(_) => Err(EvaluationError::Budget(crate::eval::limits::BudgetError {
                    kind: crate::eval::limits::BudgetKind::Materialization,
                    limit: 1,
                    consumed: 1,
                    requested: 1,
                    target: Some(completion.target),
                })),
            }
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(A1, "1", CellValue::Number(99.))
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(B1, "A1+1", CellValue::Number(88.))
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(C1, "B1+1", CellValue::Number(77.))
        .unwrap();
    let before = workbook.clone();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut transaction = Transaction {
        staged: Vec::new(),
        aborts: Vec::new(),
    };
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_observer(&mut transaction);
    let report = run.evaluate_probes(&[EvaluationProbe::Cell(target(C1))]);
    assert!(matches!(
        report.outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
    assert_eq!(transaction.staged, vec![CellValue::Number(1.)]);
    assert!(transaction.aborts.contains(&target(C1)));
    // The adapter owns this transaction; an abort discards every staged update.
    transaction.staged.clear();
    assert_eq!(workbook, before);
}

#[test]
fn observer_notification_cannot_mask_an_existing_operational_abort() {
    /// Rejects completion notifications to test preservation of an earlier failure.
    struct Refuse {
        notified: usize,
    }
    impl CellObserver for Refuse {
        fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationError> {
            self.notified += 1;
            assert!(completion.result.is_err());
            Err(EvaluationError::UnsupportedCapability {
                capability: crate::eval::limits::CapabilityId::ExternalData,
                target: Some(completion.target),
                reason: crate::eval::limits::EvaluationReasonError::ExecutionEvidenceRetentionCapacityExhausted,
            })
        }
    }
    let workbook = workbook();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut observer = Refuse { notified: 0 };
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets {
            run_work: 0,
            ..EvaluationBudgets::default()
        },
    )
    .unwrap()
    .with_cell_observer(&mut observer);
    let result = run.evaluate_cell(target(A1));
    assert!(matches!(
        run.finish(result).outcome,
        EvaluationOutcome::Aborted {
            failure: EvaluationError::Budget(_),
            ..
        }
    ));
    assert_eq!(observer.notified, 1);
}

#[test]
fn lazy_cached_reads_keep_errors_operational_and_charge_text_to_dependency() {
    /// Supplies controlled cached text or an operational failure for lazy-read tests.
    struct Cache {
        value: Arc<CellValue>,
        fail: bool,
    }
    impl CachedCellReader for Cache {
        fn cached_cell(
            &mut self,
            target: EvaluationTarget,
        ) -> Result<Option<Arc<CellValue>>, EvaluationError> {
            if target.address != C1 {
                return Ok(None);
            }
            if self.fail {
                return Err(EvaluationError::UnsupportedCapability {
                    capability: crate::eval::limits::CapabilityId::ExternalData,
                    target: Some(target),
                    reason: crate::eval::limits::EvaluationReasonError::ExternalSourceUnavailable,
                });
            }
            Ok(Some(Arc::clone(&self.value)))
        }
    }
    let mut workbook = workbook();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(A1, "1/0")
        .unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(B1, "IF(FALSE,A1,C1)")
        .unwrap();
    workbook.sheet_mut(0).unwrap().set_value(C1, 42.).unwrap();
    let policy = PreparationPolicy::default();
    let mut compiler = FormulaCompiler::new();
    let plan = compiler.compile_workbook(&workbook, &policy);
    let mut cache = Cache {
        value: Arc::new(CellValue::Number(42.)),
        fail: false,
    };
    let mut reads = Reads::default();
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_reader(&mut cache)
    .with_dependency_sink(&mut reads);
    assert_eq!(
        value(
            run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))])
                .outcome
        )[0]
        .value,
        CellValue::Number(42.)
    );
    assert!(
        !reads
            .0
            .iter()
            .any(|e| e.read == DependencyRead::Cell(target(A1)))
    );
    assert!(reads.0.contains(&ReadEvent {
        consumer: Some(target(B1)),
        read: DependencyRead::Cell(target(C1))
    }));
    workbook
        .sheet_mut(0)
        .unwrap()
        .get_mut(B1)
        .unwrap()
        .unwrap()
        .formula = Some("IFERROR(IF(FALSE,A1,C1),5)".into());
    let plan = compiler.compile_workbook(&workbook, &policy);
    cache.fail = true;
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_cell_reader(&mut cache);
    assert!(
        matches!(run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]).outcome, EvaluationOutcome::Aborted { failure: EvaluationError::UnsupportedCapability { target: Some(at), .. }, .. } if at == target(C1))
    );
    workbook
        .sheet_mut(0)
        .unwrap()
        .get_mut(B1)
        .unwrap()
        .unwrap()
        .formula = Some("IF(FALSE,A1,C1)".into());
    let large = "x".repeat(64);
    workbook
        .sheet_mut(0)
        .unwrap()
        .get_mut(C1)
        .unwrap()
        .unwrap()
        .value = CellValue::Text(large.clone());
    let before = workbook.clone();
    let plan = compiler.compile_workbook(&workbook, &policy);
    cache.value = Arc::new(CellValue::Text(large));
    cache.fail = false;
    let run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        context(),
        EvaluationBudgets {
            text_bytes: 20,
            ..EvaluationBudgets::default()
        },
    )
    .unwrap()
    .with_cell_reader(&mut cache);
    assert!(
        matches!(run.evaluate_probes(&[EvaluationProbe::Cell(target(B1))]).outcome, EvaluationOutcome::Aborted { failure: EvaluationError::Budget(crate::eval::limits::BudgetError { kind: crate::eval::limits::BudgetKind::TextBytes, target: Some(at), .. }), .. } if at == target(C1))
    );
    assert_eq!(workbook, before);
}
