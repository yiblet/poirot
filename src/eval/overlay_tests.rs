use crate::eval::run_overlay::{EffectiveWorkbook, RunInputs};
use crate::eval::shared_workbook::SharedWorkbook;
use crate::eval::{
    EvaluationBudgets, EvaluationOutcome, EvaluationRun, EvaluationTarget, FormulaCompiler,
    PreparationPolicy, RunConfiguration, WorkbookView,
};
use crate::model::query::{RangeMembership, WorkbookView as QueryView};
use crate::model::{CellAddress, CellValue, Workbook};
use std::sync::{Arc, mpsc};
use std::time::Duration;
fn at(row: u32, col: u32) -> CellAddress {
    CellAddress { row, col }
}
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("Sheet1").unwrap();
    s.set_formula_with_cached_value(at(0, 0), "99", CellValue::Number(99.))
        .unwrap();
    s.set_formula(at(0, 1), "SUM(A1:A4)").unwrap();
    s.set_formula(at(0, 2), "MATCH(7,A1:A4,0)").unwrap();
    s.set_value(at(2, 4), 23.).unwrap(); // occupied row outside requested columns
    w
}
// Independent full-consumption oracle: count page identities from source keys,
// rather than reproduce cursor stepping or effective staged-row traversal.
fn column_work(
    book: &Workbook,
    inputs: &RunInputs,
    area: RangeMembership,
) -> crate::model::query::QueryCounters {
    fn source<'a>(
        keys: impl Iterator<Item = &'a CellAddress>,
        area: RangeMembership,
        height: u32,
        width: u32,
    ) -> crate::model::query::QueryCounters {
        let mut pages = std::collections::BTreeSet::new();
        let mut visits = 0;
        for a in keys {
            if a.col / width == area.first.col / width
                && (area.first.row / height..=area.last.row / height).contains(&(a.row / height))
            {
                pages.insert(a.row / height);
            }
            if a.col == area.first.col && (area.first.row..=area.last.row).contains(&a.row) {
                visits += 1;
            }
        }
        crate::model::query::QueryCounters {
            row_seeks: pages.len() as u64,
            column_seeks: 1,
            cells_visited: visits,
            key_lookups: 0,
        }
    }
    let mut work = source(book.sheet(area.sheet).unwrap().cells().keys(), area, 32, 4);
    if let Some(input) = inputs.sheets.get(&area.sheet) {
        let extra = crate::model::query::QueryCounters {
            row_seeks: 1,
            column_seeks: 1,
            cells_visited: input
                .cells()
                .keys()
                .filter(|at| {
                    at.col == area.first.col && (area.first.row..=area.last.row).contains(&at.row)
                })
                .count() as u64,
            key_lookups: input
                .cells()
                .keys()
                .filter(|at| {
                    at.col == area.first.col && (area.first.row..=area.last.row).contains(&at.row)
                })
                .count() as u64,
        };
        work.row_seeks += extra.row_seeks;
        work.column_seeks += extra.column_seeks;
        work.cells_visited += extra.cells_visited;
        work.key_lookups += extra.key_lookups;
    }
    work
}
#[test]
fn effective_sparse_union_matches_staged_forward_reverse_and_counters() {
    let w = workbook();
    let assignments = vec![
        (0, at(0, 0), CellValue::Blank),
        (0, at(3, 0), CellValue::Number(7.)),
    ];
    let inputs = RunInputs::new(&w, assignments.clone(), 2, Arc::new(())).unwrap();
    let mut staged = w.clone();
    for (s, a, v) in assignments {
        staged.sheet_mut(s).unwrap().set_value(a, v).unwrap();
    }
    let area = RangeMembership::new(0, at(0, 0), at(5, 0)).unwrap();
    for reverse in [false, true] {
        let effective = EffectiveWorkbook::new(&w, Some(&inputs));
        let mut actual = if reverse {
            effective.stored_reverse(area)
        } else {
            effective.stored(area)
        }
        .unwrap();
        let mut expected: Vec<(CellAddress, CellValue)> = Vec::new();
        let counters = if reverse {
            let mut cells = QueryView::new(&staged).stored_reverse(area).unwrap();
            expected.extend(cells.by_ref().map(|(a, c)| (*a, c.value.clone())));
            cells.counters()
        } else {
            let mut cells = QueryView::new(&staged).stored(area).unwrap();
            expected.extend(cells.by_ref().map(|(a, c)| (*a, c.value.clone())));
            cells.counters()
        };
        assert_eq!(
            actual
                .by_ref()
                .map(|(a, c)| (*a, c.value.clone()))
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            actual.counters(),
            if reverse {
                counters
            } else {
                column_work(&w, &inputs, area)
            }
        );
    }
}
#[test]
fn formula_suppression_created_cells_defaults_and_usage_match_plain_run() {
    let w = workbook();
    let assignments = vec![
        (0, at(0, 0), CellValue::Number(-0.)),
        (0, at(3, 0), CellValue::Number(7.)),
    ];
    let inputs = RunInputs::new(&w, assignments.clone(), 2, Arc::new(())).unwrap();
    let mut staged = w.clone();
    for (s, a, v) in assignments {
        staged.sheet_mut(s).unwrap().set_value(a, v).unwrap();
    }
    let policy = PreparationPolicy::default();
    let config = RunConfiguration::default();
    let p = FormulaCompiler::new().compile_workbook(&w, &policy);
    let q = FormulaCompiler::new().compile_workbook(&staged, &policy);
    let mut actual = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &p, &policy).unwrap(),
        &config,
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_inputs(&inputs);
    let mut expected = EvaluationRun::new_with_configuration(
        WorkbookView::new(&staged, &q, &policy).unwrap(),
        &config,
        EvaluationBudgets::default(),
    )
    .unwrap();
    for address in [at(0, 0), at(0, 1), at(0, 2), at(2, 4)] {
        let target = EvaluationTarget { sheet: 0, address };
        let a = actual.evaluate_cell(target).unwrap();
        let b = expected.evaluate_cell(target).unwrap();
        assert!(crate::eval::same_result_identity(&a, &b));
    }
    assert_eq!(actual.usage(), expected.usage());
    let actual = actual.finish(Ok(()));
    let expected = expected.finish(Ok(()));
    match (actual.outcome, expected.outcome) {
        (
            EvaluationOutcome::Complete { usage: a, .. },
            EvaluationOutcome::Complete { usage: b, .. },
        ) => {
            // SUM and MATCH each add an input column bound seek, a page seek,
            // and the physical overridden A1 visit relative to staged storage.
            let mut a = a;
            a.work_units -= 6;
            a.sparse_entries -= 6;
            assert_eq!(a, b);
        }
        _ => panic!("expected complete runs"),
    }
    assert!(w.sheet(0).unwrap().get(at(0, 0)).unwrap().formula.is_some());
    assert!(w.sheet(0).unwrap().get(at(3, 0)).is_none());
}
#[test]
fn shared_runs_share_immutable_source_and_keep_memos_isolated() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SharedWorkbook>();
    let w = workbook();
    let plan = Arc::new(FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default()));
    let shared = SharedWorkbook::new(w, plan).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let mut workers = Vec::new();
    let mut release = Vec::new();
    for number in [3., 8.] {
        let shared = shared.clone();
        let started = started_tx.clone();
        let (tx, rx) = mpsc::channel();
        release.push(tx);
        workers.push(std::thread::spawn(move || {
            let inputs = shared
                .prepare_inputs(|_| {
                    Ok::<_, crate::model::ModelError>((
                        (),
                        vec![(0, at(0, 0), CellValue::Number(number))],
                        1,
                    ))
                })
                .unwrap()
                .1;
            shared
                .run_workbook(
                    &RunConfiguration::default(),
                    EvaluationBudgets::default(),
                    &inputs,
                    |run, _| {
                        started.send(()).unwrap();
                        rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        run.evaluate_cell(EvaluationTarget {
                            sheet: 0,
                            address: at(0, 1),
                        })
                    },
                )
                .unwrap()
        }));
    }
    for _ in 0..2 {
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    for tx in release {
        tx.send(()).unwrap();
    }
    for (worker, number) in workers.into_iter().zip([3., 8.]) {
        let result = worker.join().unwrap();
        // Range consumers borrow the authored input instead of retaining a copy.
        assert!(!result.cells.contains_key(&(0, at(0, 0))));
        assert_eq!(
            result.cells.get(&(0, at(0, 1))),
            Some(&CellValue::Number(number))
        );
        match result.report.outcome {
            EvaluationOutcome::Complete { value, .. } => {
                assert_eq!(value, CellValue::Number(number))
            }
            _ => panic!("complete run expected"),
        }
    }
    shared.with_source(|w| {
        assert_eq!(
            w.sheet(0).unwrap().get(at(0, 0)).unwrap().value,
            CellValue::Number(99.)
        );
        assert!(w.sheet(0).unwrap().get(at(0, 0)).unwrap().formula.is_some());
    });
    let empty = shared
        .prepare_inputs(|_| Ok::<_, crate::model::ModelError>(((), vec![], 0)))
        .unwrap()
        .1;
    let result = shared
        .run_workbook(
            &RunConfiguration::default(),
            EvaluationBudgets::default(),
            &empty,
            |run, _| {
                run.evaluate_cell(EvaluationTarget {
                    sheet: 0,
                    address: at(0, 1),
                })
            },
        )
        .unwrap();
    assert_eq!(
        result.cells.get(&(0, at(0, 0))),
        Some(&CellValue::Number(99.))
    );
}
#[test]
fn aborted_run_does_not_change_baseline_or_seed_next_memo() {
    let w = workbook();
    let plan = Arc::new(FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default()));
    let shared = SharedWorkbook::new(w, plan).unwrap();
    let inputs = shared
        .prepare_inputs(|_| Ok::<_, crate::model::ModelError>(((), vec![], 0)))
        .unwrap()
        .1;
    let result = shared
        .run_workbook(
            &RunConfiguration::default(),
            EvaluationBudgets {
                run_work: 1,
                ..EvaluationBudgets::default()
            },
            &inputs,
            |run, _| {
                run.evaluate_cell(EvaluationTarget {
                    sheet: 0,
                    address: at(0, 1),
                })
            },
        )
        .unwrap();
    assert!(matches!(
        result.report.outcome,
        EvaluationOutcome::Aborted { .. }
    ));
    shared.with_source(|w| {
        assert_eq!(
            w.sheet(0).unwrap().get(at(0, 1)).unwrap().value,
            CellValue::Blank
        )
    });
    let next = shared
        .run_workbook(
            &RunConfiguration::default(),
            EvaluationBudgets::default(),
            &inputs,
            |run, _| {
                run.evaluate_cell(EvaluationTarget {
                    sheet: 0,
                    address: at(0, 1),
                })
            },
        )
        .unwrap();
    assert!(matches!(
        next.report.outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Number(99.),
            ..
        }
    ));
}
#[test]
fn equal_cached_assignment_still_suppresses_formula_and_stale_baseline_is_refused() {
    let mut w = workbook();
    let policy = PreparationPolicy::default();
    let plan = Arc::new(FormulaCompiler::new().compile_workbook(&w, &policy));
    let inputs =
        RunInputs::new(&w, [(0, at(0, 0), CellValue::Number(99.))], 1, Arc::new(())).unwrap();
    let config = RunConfiguration::default();
    let mut run = EvaluationRun::new_with_configuration(
        WorkbookView::new(&w, &plan, &policy).unwrap(),
        &config,
        EvaluationBudgets::default(),
    )
    .unwrap()
    .with_inputs(&inputs);
    assert_eq!(
        run.evaluate_cell(EvaluationTarget {
            sheet: 0,
            address: at(0, 0)
        })
        .unwrap(),
        CellValue::Number(99.)
    );
    assert_eq!(run.usage().formula_executions, 0);
    drop(run);
    w.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(at(0, 0), "100", CellValue::Number(99.))
        .unwrap();
    assert!(matches!(
        SharedWorkbook::new(w, plan),
        Err(crate::eval::EvaluationFailure::StalePlan)
    ));
}
#[test]
fn source_bound_inputs_reject_same_topology_different_source() {
    let w = workbook();
    let plan = Arc::new(FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default()));
    let other = SharedWorkbook::new(w.clone(), plan.clone()).unwrap();
    let shared = SharedWorkbook::new(w, plan).unwrap();
    let inputs = shared
        .prepare_inputs(|_| Ok::<_, crate::model::ModelError>(((), vec![], 0)))
        .unwrap()
        .1;
    assert!(matches!(
        other.run_workbook(
            &RunConfiguration::default(),
            EvaluationBudgets::default(),
            &inputs,
            |_, _| Ok(())
        ),
        Err(crate::eval::shared_workbook::SharedRunFailure::ForeignInputs)
    ));
    assert!(
        shared
            .clone()
            .run_workbook(
                &RunConfiguration::default(),
                EvaluationBudgets::default(),
                &inputs,
                |_, _| Ok(())
            )
            .is_ok()
    );
}
#[test]
fn shared_source_admits_inputs_and_preserves_observed_run_results() {
    use crate::eval::execution_observation::{ExecutionEvent, ExecutionObserver};
    use crate::eval::run::{DependencyReadSink, MemoRetentionLimits, ReadEvent, RunObservers};
    use crate::eval::{BuiltinInvocationObserver, CellCompletion, CellObserver, EvaluationFailure};

    /// Counts delivered events without retaining borrowed runtime data.
    #[derive(Default)]
    struct Events(usize);
    impl DependencyReadSink for Events {
        fn read(&mut self, _: ReadEvent) {
            self.0 += 1;
        }
    }
    impl CellObserver for Events {
        fn completed(&mut self, _: CellCompletion<'_>) -> Result<(), EvaluationFailure> {
            self.0 += 1;
            Ok(())
        }
    }
    impl BuiltinInvocationObserver for Events {
        fn consumed(
            &mut self,
            _: crate::eval::ConsumedBuiltinInvocation<'_>,
        ) -> Result<(), EvaluationFailure> {
            self.0 += 1;
            Ok(())
        }
    }
    impl ExecutionObserver for Events {
        fn observe(&mut self, _: ExecutionEvent<'_>) -> Result<(), EvaluationFailure> {
            self.0 += 1;
            Ok(())
        }
    }

    let w = workbook();
    let plan = Arc::new(FormulaCompiler::new().compile_workbook(&w, &PreparationPolicy::default()));
    let shared = SharedWorkbook::new(w, plan).unwrap();
    assert!(shared.with_prepared(|w, p| WorkbookView::new(w, p, p.policy()).is_ok()));
    let invalid = shared.prepare_inputs(|_| {
        Ok::<_, crate::model::ModelError>(((), vec![(1, at(0, 0), CellValue::Blank)], 1))
    });
    assert!(invalid.is_err());
    let (_, inputs) = shared
        .prepare_inputs(|_| {
            Ok::<_, crate::model::ModelError>(((), vec![(0, at(0, 0), CellValue::Number(5.))], 1))
        })
        .unwrap();
    let (mut reads, mut cells, mut builtins, mut operations) = (
        Events::default(),
        Events::default(),
        Events::default(),
        Events::default(),
    );
    let result = shared
        .run_workbook_observed(
            &RunConfiguration::default(),
            EvaluationBudgets::default(),
            &inputs,
            MemoRetentionLimits::default(),
            RunObservers {
                reads: &mut reads,
                cells: &mut cells,
                builtins: &mut builtins,
                operations: &mut operations,
            },
            |run, _| {
                let target = EvaluationTarget {
                    sheet: 0,
                    address: at(0, 1),
                };
                assert_eq!(run.evaluate_cell(target)?, CellValue::Number(5.));
                run.evaluate_cell(target)
            },
        )
        .unwrap();
    assert!(matches!(
        result.report.outcome,
        EvaluationOutcome::Complete {
            value: CellValue::Number(5.),
            ..
        }
    ));
    assert_eq!(
        result.cells.get(&(0, at(0, 1))),
        Some(&CellValue::Number(5.))
    );
    assert!(reads.0 > 0 && cells.0 > 0 && builtins.0 > 0 && operations.0 > 0);
    assert_eq!(
        shared.with_source(|w| w.sheet(0).unwrap().get(at(0, 0)).unwrap().value.clone()),
        CellValue::Number(99.)
    );
}

#[test]
fn sparse_union_exhausts_small_topologies_boundaries_and_partial_reads() {
    for base_mask in 0u8..16 {
        for overlay_mask in 0u8..16 {
            let mut base = Workbook::new();
            base.add_sheet("S").unwrap();
            let mut assignments = Vec::new();
            for row in 0..4 {
                if base_mask & (1 << row) != 0 {
                    base.sheet_mut(0)
                        .unwrap()
                        .set_value(at(row, row % 3), row as f64)
                        .unwrap();
                }
                if overlay_mask & (1 << row) != 0 {
                    assignments.push((0, at(row, (row + 1) % 3), CellValue::Blank));
                    if base_mask & (1 << row) != 0 {
                        assignments.push((0, at(row, row % 3), CellValue::Number(-0.)));
                    }
                }
            }
            let inputs = RunInputs::new(&base, assignments.clone(), 8, Arc::new(())).unwrap();
            let mut staged = base.clone();
            for (s, a, v) in assignments {
                staged.sheet_mut(s).unwrap().set_value(a, v).unwrap();
            }
            for first_row in 0..4 {
                for last_row in first_row..5 {
                    for (first_col, last_col) in [(0, 0), (1, 1), (2, 2), (0, 2)] {
                        let area = RangeMembership::new(
                            0,
                            at(first_row, first_col),
                            at(last_row, last_col),
                        )
                        .unwrap();
                        for reverse in [false, true] {
                            let effective = EffectiveWorkbook::new(&base, Some(&inputs));
                            let mut actual = if reverse {
                                effective.stored_reverse(area)
                            } else {
                                effective.stored(area)
                            }
                            .unwrap();
                            // Both concrete query iterator types share the same Item.
                            let (expected, counters) = if reverse {
                                let mut iter =
                                    QueryView::new(&staged).stored_reverse(area).unwrap();
                                let values = iter
                                    .by_ref()
                                    .map(|(a, c)| (*a, c.value.clone()))
                                    .collect::<Vec<_>>();
                                (values, iter.counters())
                            } else {
                                let mut iter = QueryView::new(&staged).stored(area).unwrap();
                                let values = iter
                                    .by_ref()
                                    .map(|(a, c)| (*a, c.value.clone()))
                                    .collect::<Vec<_>>();
                                (values, iter.counters())
                            };
                            let values = actual
                                .by_ref()
                                .map(|(a, c)| (*a, c.value.clone()))
                                .collect::<Vec<_>>();
                            assert_eq!(values, expected);
                            let expected_work = if !reverse && first_col == last_col {
                                column_work(&base, &inputs, area)
                            } else {
                                counters
                            };
                            assert_eq!(actual.counters(), expected_work);
                            assert!(actual.next().is_none());
                            assert_eq!(actual.counters(), expected_work);
                        }
                    }
                }
            }
        }
    }
}
