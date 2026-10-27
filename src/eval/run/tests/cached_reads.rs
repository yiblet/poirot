//! Cache reads preserve worksheet values, ownership observations and metered failures.
use super::super::evaluation_run::EvaluationRun;
use super::super::workbook_view::WorkbookView;
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::EvaluationContext;
use crate::eval::config::{ArrayResultPolicy, RunConfiguration};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    BudgetFailure, BudgetKind, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    EvaluationUsage,
};
use crate::eval::overlay;
use crate::eval::ports::{CachedCellReader, CellCompletion, CellObserver, CellProvenance};
use crate::eval::ports::{DependencyRead, DependencyReadSink, ReadEvent};
use crate::model::{CellError, CellValue, Workbook};
use std::sync::Arc;

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

#[test]
fn direct_cleanup_export_is_ordered_and_survives_cache_clears() {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_sheet("Sheet1").unwrap();
    for address in ["B1", "A2", "Z20"] {
        worksheet
            .set_formula(address.parse().unwrap(), "1+1")
            .unwrap();
    }
    let mut evaluation = overlay::Evaluation::with_workbook(&workbook)
        .build(Default::default())
        .unwrap();
    for address in ["Z20", "A2", "B1"] {
        assert_eq!(evaluation.cell(target(address)).unwrap(), 2.0.into());
    }
    let cleanup = |changes: Vec<crate::model::WorkbookChange>| {
        changes
            .into_iter()
            .filter_map(|change| match change {
                crate::model::WorkbookChange::ClearSpillCache { sheet, anchor } => {
                    Some(EvaluationTarget {
                        sheet,
                        address: anchor,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let expected = vec![target("B1"), target("A2"), target("Z20")];
    assert_eq!(cleanup(evaluation.changes()), expected);
    evaluation.clear_calculated_cache();
    assert_eq!(cleanup(evaluation.changes()), expected);
    evaluation.discard_edits();
    assert!(cleanup(evaluation.changes()).is_empty());
}

/// Capture every completion, including operational abort provenance.
#[derive(Default)]
struct Completions(
    Vec<(
        EvaluationTarget,
        CellProvenance,
        Result<CellValue, EvaluationFailure>,
    )>,
);

impl CellObserver for Completions {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0.push((
            event.target,
            event.provenance,
            event.result.cloned().map_err(Clone::clone),
        ));
        Ok(())
    }
}

/// Capture dependency edges from both initial and memoized spill children.
#[derive(Default)]
struct Reads(Vec<ReadEvent>);

impl DependencyReadSink for Reads {
    fn read(&mut self, event: ReadEvent) {
        self.0.push(event);
    }
}

#[test]
fn local_memos_keep_scalar_values_and_spill_owner_edges() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Sheet1").unwrap();
    sheet.set_value(target("A1").address, 7.).unwrap();
    sheet.set_value(target("A2").address, "text").unwrap();
    sheet.set_value(target("A3").address, true).unwrap();
    sheet
        .set_formula_with_cached_value(target("B1").address, "A1+1", CellValue::Number(99.))
        .unwrap();
    sheet.set_formula(target("B2").address, "1/0").unwrap();
    sheet.set_formula(target("D1").address, "{1,2}").unwrap();
    sheet.set_formula(target("G1").address, "E1+E1").unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let mut completions = Completions::default();
    let mut reads = Reads::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap()
    .with_cell_observer(&mut completions)
    .with_dependency_sink(&mut reads);
    // Expectations are literal values, ordinary addition and row-array coordinates.
    for (at, expected) in [
        ("A1", CellValue::Number(7.)),
        ("A2", CellValue::from("text")),
        ("A3", CellValue::Boolean(true)),
        ("A4", CellValue::Blank),
        ("B1", CellValue::Number(8.)),
        ("B2", CellValue::Error(CellError::Div0)),
        ("E1", CellValue::Number(2.)),
    ] {
        assert_eq!(run.evaluate_cell(target(at)).unwrap(), expected);
        assert_eq!(run.evaluate_cell(target(at)).unwrap(), expected);
    }
    assert_eq!(
        run.evaluate_cell(target("G1")).unwrap(),
        CellValue::Number(4.)
    );
    let report = run.finish(Ok(()));
    assert!(report.execution.session_memo_hits >= 8);
    for at in ["A1", "A2", "A3", "A4", "B1", "B2", "E1"] {
        assert!(
            completions
                .0
                .iter()
                .any(|(at2, provenance, result)| *at2 == target(at)
                    && *provenance == CellProvenance::SessionMemo
                    && result.is_ok())
        );
    }
    assert_eq!(
        reads
            .0
            .iter()
            .filter(|event| event.consumer == Some(target("G1"))
                && event.read == DependencyRead::Cell(target("D1")))
            .count(),
        2
    );
}

#[test]
fn memoized_text_failure_retains_target_provenance_usage_and_frame_unwinding() {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Sheet1").unwrap();
    sheet.set_value(target("A1").address, "abcdef").unwrap();
    sheet.set_value(target("B1").address, 9.).unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut completions = Completions::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        Default::default(),
        EvaluationBudgets {
            run_work: 20,
            cell_work: 13,
            ..Default::default()
        },
    )
    .unwrap()
    .with_cell_observer(&mut completions);
    assert_eq!(
        run.evaluate_cell(target("A1")).unwrap(),
        CellValue::from("abcdef")
    );
    let failure = run.evaluate_cell(target("A1")).unwrap_err();
    // Initial read copies six text bytes into the result and again into the memo,
    // and admits one memo value. Authored nonblank A1 blocks other spills, so
    // preparation needs no sparse inspection (14 units total). A memo read costs one more; its six
    // text bytes exceed twenty. The rejected charge is atomic.
    assert_eq!(
        failure,
        EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::RunWork,
            limit: 20,
            consumed: 15,
            requested: 6,
            target: Some(target("A1"))
        })
    );
    // A refused cycle stays failed; no new read or observer event is performed.
    assert_eq!(run.evaluate_cell(target("B1")).unwrap_err(), failure);
    let report = run.finish(Err::<(), _>(failure.clone()));
    assert_eq!(report.execution.session_memo_hits, 1);
    assert!(matches!(
        report.outcome,
        EvaluationOutcome::Aborted {
            usage: EvaluationUsage {
                work_units: 15,
                cell_reads: 2,
                text_bytes: 12,
                ..
            },
            ..
        }
    ));
    assert!(
        completions
            .0
            .contains(&(target("A1"), CellProvenance::SessionMemo, Err(failure)))
    );
}

/// Supply an immutable completed text result through the public cache boundary.
struct TextCache;

impl CachedCellReader for TextCache {
    fn cached_cell(
        &mut self,
        _: EvaluationTarget,
    ) -> Result<Option<Arc<CellValue>>, EvaluationFailure> {
        Ok(Some(Arc::new(CellValue::from("abcdef"))))
    }
}

#[test]
fn external_cached_text_is_checked_before_copy_and_observed_on_abort() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let mut cache = TextCache;
    let mut completions = Completions::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        Default::default(),
        EvaluationBudgets {
            text_bytes: 5,
            ..Default::default()
        },
    )
    .unwrap()
    .with_cell_reader(&mut cache)
    .with_cell_observer(&mut completions);
    let failure = run.evaluate_cell(target("A1")).unwrap_err();
    assert_eq!(
        failure,
        EvaluationFailure::Budget(BudgetFailure {
            kind: BudgetKind::TextBytes,
            limit: 5,
            consumed: 0,
            requested: 6,
            target: Some(target("A1"))
        })
    );
    let report = run.finish(Err::<(), _>(failure.clone()));
    assert_eq!(report.execution.external_cache_hits, 1);
    assert_eq!(
        completions.0,
        vec![(target("A1"), CellProvenance::ExternalCache, Err(failure))]
    );
}

/// Record the observable order shared by dependency and completion callbacks.
#[derive(Clone)]
struct OrderedReads(std::rc::Rc<std::cell::RefCell<Vec<String>>>);

impl DependencyReadSink for OrderedReads {
    fn read(&mut self, event: ReadEvent) {
        if let DependencyRead::Cell(target) = event.read {
            self.0.borrow_mut().push(format!("read:{}", target.address));
        }
    }
}

impl CellObserver for OrderedReads {
    fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
        self.0.borrow_mut().push(format!(
            "completed:{}:{:?}",
            event.target.address, event.provenance
        ));
        Ok(())
    }
}

#[test]
fn memoized_spill_child_reports_its_owner_before_completion() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Sheet1")
        .unwrap()
        .set_formula(target("D1").address, "{1,2}")
        .unwrap();
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::DynamicSpill);
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut reads = OrderedReads(events.clone());
    let mut completions = reads.clone();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&workbook, &plan, &policy).unwrap(),
        &configuration,
        Default::default(),
    )
    .unwrap()
    .with_dependency_sink(&mut reads)
    .with_cell_observer(&mut completions);
    // A two-column row array places its second element immediately right of D1.
    assert_eq!(
        run.evaluate_cell(target("E1")).unwrap(),
        CellValue::Number(2.)
    );
    events.borrow_mut().clear();
    assert_eq!(
        run.evaluate_cell(target("E1")).unwrap(),
        CellValue::Number(2.)
    );
    assert_eq!(
        *events.borrow(),
        ["read:E1", "read:D1", "completed:E1:SessionMemo"]
    );
}

#[test]
fn overlay_cached_changes_keep_sheet_and_row_order_after_snapshot_restore() {
    let mut workbook = Workbook::new();
    for name in ["First", "Second"] {
        let sheet = workbook.add_sheet(name).unwrap();
        for address in ["Z20", "A2", "B1"] {
            sheet.set_formula(address.parse().unwrap(), "2+3").unwrap();
        }
    }
    let mut overlay = overlay::Overlay::with_workbook(&workbook).build().unwrap();
    let context = EvaluationContext::default();
    for sheet in [1, 0] {
        for address in ["Z20", "A2", "B1"] {
            assert_eq!(
                overlay
                    .evaluation(&context)
                    .unwrap()
                    .cell(EvaluationTarget {
                        sheet,
                        address: address.parse().unwrap(),
                    })
                    .unwrap(),
                CellValue::Number(5.)
            );
        }
    }
    let cached = |changes: Vec<crate::model::WorkbookChange>| {
        changes
            .into_iter()
            .filter_map(|change| match change {
                crate::model::WorkbookChange::SetCachedValue { sheet, address, .. } => {
                    Some((sheet, address))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let expected = [0, 1]
        .into_iter()
        .flat_map(|sheet| ["B1", "A2", "Z20"].map(|address| (sheet, address.parse().unwrap())))
        .collect::<Vec<_>>();
    assert_eq!(cached(overlay.changes()), expected);
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (restored_workbook, state) =
        overlay::Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&restored_workbook).unwrap();
    assert_eq!(cached(restored.changes()), expected);
}
