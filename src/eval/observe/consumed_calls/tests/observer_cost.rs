//! Calculation admission is independent of nonrefusing diagnostic ports.
use super::support::{A1, B1, Bounded, names, plan, workbook};
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetKind, EvaluationFailure};
use crate::eval::observe::execution::{ExecutionEvent, ExecutionObserver};
use crate::eval::ports::{CellCompletion, CellObserver, DependencyReadSink, ReadEvent};
use crate::eval::session::{CellEvaluator, FormulaExecution, Session};
use crate::model::{CellValue, Workbook};

/// Collects consumed reads without refusing diagnostic retention.
#[derive(Default)]
struct Reads(Vec<ReadEvent>);

impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

/// Counts completed cells without changing calculation admission.
#[derive(Default)]
struct Cells(usize);

impl CellObserver for Cells {
    fn completed(&mut self, _: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0 += 1;
        Ok(())
    }
}

/// Counts paired operation entries and completions.
#[derive(Default)]
struct Operations {
    begins: usize,
    completions: usize,
}

impl ExecutionObserver for Operations {
    fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationFailure> {
        match event {
            ExecutionEvent::Begin(_) => self.begins += 1,
            ExecutionEvent::Complete { .. } => self.completions += 1,
            ExecutionEvent::MaskDecision { .. } => {}
        }
        Ok(())
    }
}

/// Counts external-cache queries while always requesting ordinary evaluation.
#[derive(Default)]
struct CacheReads(usize);

impl crate::eval::ports::CachedCellReader for CacheReads {
    fn cached_cell(
        &mut self,
        _: EvaluationTarget,
    ) -> Result<Option<std::sync::Arc<CellValue>>, EvaluationFailure> {
        self.0 += 1;
        Ok(None)
    }
}

/// Captures results, work, retained memo, and random state for observer-cost comparisons.
#[derive(Debug, PartialEq)]
struct Snapshot {
    results: Vec<CellValue>,
    calculation: crate::eval::limits::EvaluationUsage,
    execution: crate::eval::limits::RunUsage,
    memo: crate::eval::limits::MemoRetentionUsage,
    cache: crate::eval::coordinates::CellMemo,
    next_random: u64,
}

fn fixture() -> Workbook {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet.set_formula(A1, "RAND()").unwrap();
    sheet.set_formula(B1, "IF(TRUE,ABS(-2)+A1+A1,0)").unwrap();
    workbook
}

fn calculate(
    workbook: &Workbook,
    budgets: crate::eval::limits::EvaluationBudgets,
    observed: bool,
) -> Snapshot {
    let mut calls = Bounded::new(20, 4096);
    let mut reads = Reads::default();
    let mut cells = Cells::default();
    let mut operations = Operations::default();
    let mut cache_reads = CacheReads::default();
    let mut session = Session::new(
        workbook,
        EvaluationInputs {
            randomness_seed: 137,
            ..EvaluationInputs::default()
        },
        crate::eval::limits::BudgetMeter::new(budgets).unwrap(),
    );
    if observed {
        session.ports.builtin_invocation_observer = Some(&mut calls);
        session.ports.dependency_sink = Some(&mut reads);
        session.ports.cell_observer = Some(&mut cells);
        session.ports.execution_observer = Some(&mut operations);
        session.ports.cell_reader = Some(&mut cache_reads);
    }
    // Dependency evaluation and repeated targets share this actual Session.
    let results = vec![
        CellEvaluator::new(&mut session).cell(0, B1).unwrap(),
        CellEvaluator::new(&mut session).cell(0, B1).unwrap(),
        CellEvaluator::new(&mut session).cell(0, A1).unwrap(),
    ];
    assert_eq!(session.state.runtime.effects.random_streams.len(), 1);
    let snapshot = Snapshot {
        results,
        calculation: session.state.runtime.budget.usage.clone(),
        execution: session.state.runtime.run_usage.clone(),
        memo: session.state.runtime.memo.usage,
        cache: session.state.runtime.memo.cache.clone(),
        // Check retained RNG state as well as the metered draw receipts.
        next_random: session
            .state
            .runtime
            .effects
            .random_streams
            .get_mut(&(0, A1))
            .unwrap()
            .0
            .unit()
            .to_bits(),
    };
    drop(session);
    if observed {
        assert_eq!(names(&calls), ["IF", "ABS", "RAND"]);
        assert!(!reads.0.is_empty());
        assert_eq!(cells.0, 5);
        assert_eq!(cache_reads.0, 2);
        assert!(operations.begins > 0);
        assert_eq!(operations.begins, operations.completions);
        assert!(
            reads
                .0
                .iter()
                .any(|event| matches!(&event.read, crate::eval::ports::DependencyRead::Random(_)))
        );
    }
    snapshot
}

#[test]
fn all_ports_preserve_calculation_usage_memo_execution_and_rng() {
    let workbook = fixture();
    let baseline = calculate(
        &workbook,
        crate::eval::limits::EvaluationBudgets::default(),
        false,
    );
    assert_eq!(baseline.execution.formula_executions, 2);
    assert_eq!(baseline.execution.session_memo_hits, 3);
    assert_eq!(baseline.calculation.random_accesses.len(), 1);
    assert_eq!(baseline.calculation.random_accesses[0].draws, 1);
    assert!(baseline.calculation.text_bytes > 0);
    let observed = calculate(
        &workbook,
        crate::eval::limits::EvaluationBudgets::default(),
        true,
    );
    assert_eq!(observed, baseline);
    // A calculation admitted at its exact work bound still completes with ports.
    let exact = crate::eval::limits::EvaluationBudgets {
        run_work: baseline.calculation.work_units,
        ..crate::eval::limits::EvaluationBudgets::default()
    };
    assert_eq!(calculate(&workbook, exact, false), baseline);
    assert_eq!(calculate(&workbook, exact, true), baseline);
}

#[test]
fn calculation_expression_admission_and_owned_observer_admission_stay_separate() {
    let workbook = workbook();
    let formula = plan("RAND()");
    for (work, observer_bytes, expected) in [
        (0, 4096, BudgetKind::RunWork),
        (u64::MAX, 0, BudgetKind::TextBytes),
    ] {
        let mut observer = Bounded::new(20, observer_bytes);
        let mut session = Session::new(
            &workbook,
            EvaluationInputs::default(),
            crate::eval::limits::BudgetMeter::new(crate::eval::limits::EvaluationBudgets {
                run_work: work,
                text_bytes: 0,
                ..crate::eval::limits::EvaluationBudgets::default()
            })
            .unwrap(),
        );
        session.ports.builtin_invocation_observer = Some(&mut observer);
        let failure = FormulaExecution::new(&mut session)
            .bound_at(
                EvaluationTarget {
                    sheet: 0,
                    address: A1,
                },
                &formula,
            )
            .unwrap_err();
        assert_eq!(failure.budget().unwrap().kind, expected);
        assert!(session.state.runtime.effects.random_streams.is_empty());
        assert!(
            session
                .state
                .runtime
                .budget
                .usage
                .random_accesses
                .is_empty()
        );
        assert_eq!(session.state.runtime.budget.usage.materialized_values, 0);
        assert_eq!(session.state.runtime.budget.usage.text_bytes, 0);
        drop(session);
        assert!(observer.entries.is_empty());
        assert_eq!(observer.retained_bytes, 0);
    }
}
