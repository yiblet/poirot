//! Original proofs of Poirot's demand-local preparation and explicit top-left policy.
//! Array coordinates follow the documented down/right layout; overlap priority is
//! a Poirot contract, not an asserted Excel tie-break.
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::eval::overlay::{Evaluation, Overlay};
use crate::eval::ports::{CellCompletion, CellObserver, CellProvenance};
use crate::model::{CellError, CellValue, Workbook};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}

fn workbook(formulas: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, formula) in formulas {
        sheet
            .set_formula(target(address).address, *formula)
            .unwrap();
    }
    book
}

fn verify(book: &Workbook, expected: &[(&str, CellValue)]) {
    for reverse in [false, true] {
        let mut targets = expected.to_vec();
        if reverse {
            targets.reverse();
        }
        let mut direct = Evaluation::with_workbook(book)
            .build(Default::default())
            .unwrap();
        let mut overlay = Overlay::with_workbook(book).build().unwrap();
        let context = EvaluationContext::default();
        for _ in 0..2 {
            let mut run = overlay.evaluation(&context).unwrap();
            for (address, value) in &targets {
                assert_eq!(
                    direct.cell(target(address)).unwrap(),
                    *value,
                    "direct {address}"
                );
                assert_eq!(
                    run.cell(target(address)).unwrap(),
                    *value,
                    "overlay {address}"
                );
            }
        }
    }
}

#[test]
fn proved_dimensions_and_direction_exclude_unrelated_producers() {
    let mut book = workbook(&[
        ("A1", "SEQUENCE(3)"),
        ("D5", "SEQUENCE(G1)"),
        ("J1", "{1,2}+K1"),
    ]);
    book.sheet_mut(0)
        .unwrap()
        .set_value(target("G1").address, 3.0)
        .unwrap();
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    for address in ["A4", "D4", "E6", "L1"] {
        assert_eq!(run.cell(target(address)).unwrap(), CellValue::Blank);
    }
    assert_eq!(run.usage().formula_executions, 0);
    assert_eq!(run.cell(target("A3")).unwrap(), 3.0.into());
    assert_eq!(run.usage().formula_executions, 1);
    assert_eq!(run.cell(target("D6")).unwrap(), 2.0.into());
    assert_eq!(run.usage().formula_executions, 2);
}

#[test]
fn an_indirect_competitor_wins_even_when_it_does_not_cover_the_demand() {
    // B1:C2 beats A2:B3 at B2. B1's rectangle does not include requested A3.
    let book = workbook(&[("B1", "{10,11;12,13}"), ("A2", "{1,2;3,4}")]);
    verify(
        &book,
        &[
            ("A3", CellValue::Blank),
            ("A2", CellValue::Error(CellError::Spill)),
            ("B2", 12.0.into()),
            ("C2", 13.0.into()),
        ],
    );
}

#[test]
fn possible_overlap_does_not_block_disjoint_actual_footprints() {
    let mut book = workbook(&[
        ("D1", "IF(G1,{1;2},{1;2;3;4;5})+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    book.sheet_mut(0)
        .unwrap()
        .set_value(target("G1").address, true)
        .unwrap();
    verify(
        &book,
        &[
            ("D1", 21.0.into()),
            ("D2", 22.0.into()),
            ("A6", 20.0.into()),
            ("D5", 13.0.into()),
        ],
    );
}

#[test]
fn settled_supplier_displacement_produces_only_final_observer_values() {
    use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
    use crate::eval::run::{EvaluationRun, WorkbookView};
    /// Retain committed cell values for the notification contract.
    #[derive(Default)]
    struct Observer(Vec<(EvaluationTarget, CellProvenance, CellValue)>);
    impl CellObserver for Observer {
        fn completed(&mut self, event: CellCompletion<'_>) -> Result<(), EvaluationError> {
            if let Ok(value) = event.result {
                self.0.push((event.target, event.provenance, value.clone()));
            }
            Ok(())
        }
    }
    let book = workbook(&[
        ("D1", "{1;2;3;4;5}+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut observer = Observer::default();
    let mut run = EvaluationRun::new(
        WorkbookView::new(&book, &plan, &policy).unwrap(),
        Default::default(),
        Default::default(),
    )
    .unwrap()
    .with_cell_observer(&mut observer);
    assert_eq!(run.evaluate_cell(target("D5")).unwrap(), 5.0.into());
    run.finish(Ok(()));
    assert!(
        observer
            .0
            .iter()
            .any(|(t, _, v)| *t == target("A5") && *v == CellValue::Error(CellError::Spill))
    );
    assert!(
        observer
            .0
            .iter()
            .filter(|(t, _, _)| *t == target("A6"))
            .all(|(_, _, v)| *v == CellValue::Blank)
    );
    assert!(
        observer
            .0
            .iter()
            .filter(|(t, _, _)| *t == target("D1"))
            .all(|(_, _, v)| *v == 1.0.into())
    );
}

#[test]
fn captured_random_draw_is_rewound_when_ownership_requires_recomputation() {
    let reference = workbook(&[("D1", "{1;2;3;4;5}+RAND()")]);
    let competing = workbook(&[
        ("D1", "{1;2;3;4;5}+RAND()+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    let mut baseline = Evaluation::with_workbook(&reference)
        .build(Default::default())
        .unwrap();
    let mut run = Evaluation::with_workbook(&competing)
        .build(Default::default())
        .unwrap();
    for address in ["D1", "D2", "D5"] {
        assert_eq!(
            run.cell(target(address)).unwrap(),
            baseline.cell(target(address)).unwrap()
        );
    }
}

#[test]
fn oscillating_actual_geometry_refuses_to_publish_a_false_fixed_point() {
    // With A5 present, D1 is tall and defeats A5; without A5, D1 is short
    // and A5 fits. Neither ownership assignment is a stable result.
    let book = workbook(&[
        ("D1", "IF(SUM(A6)=0,{1;2},{1;2;3;4;5})"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    let mut run = overlay.evaluation(&context).unwrap();
    let failure = run.cell(target("D1")).unwrap_err();
    assert!(
        matches!(failure,EvaluationError::UnsupportedCapability { reason,.. } if matches!(reason, crate::eval::limits::EvaluationReasonError::SpillOwnershipComponentDoesNotSettle))
    );
    assert_eq!(run.values().count(), 0);
    assert!(run.result_changes().unwrap().is_empty());
}

#[test]
fn external_effects_are_not_replayed_but_unused_external_branches_are_harmless() {
    use crate::eval::functions::extensions::*;
    use crate::eval::functions::registry::Arity;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for cacheability in [
        ExtensionCacheability::Opaque,
        ExtensionCacheability::Volatile,
        ExtensionCacheability::Deterministic,
    ] {
        for unused in [false, true] {
            let calls = Arc::new(AtomicUsize::new(0));
            let counter = calls.clone();
            let mut registry = ExtensionRegistry::new();
            registry
                .register(
                    ExtensionNamespace::UserDefined,
                    "SIDE",
                    "test/v1",
                    ExtensionContract {
                        arity: Arity {
                            minimum: 0,
                            maximum: 0,
                        },
                        evaluation: ExtensionEvaluationStrategy::Eager,
                        capabilities: ExtensionCapabilities {
                            arrays: false,
                            references: false,
                            context: false,
                        },
                        cacheability,
                    },
                    move |_: &mut dyn ExtensionHost| {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok(ExtensionOutput::Scalar(0.0.into()))
                    },
                )
                .unwrap();
            let term = if unused {
                "IF(TRUE,0,SIDE())"
            } else {
                "SIDE()"
            };
            let book = workbook(&[
                ("D1", &format!("{{1;2;3;4;5}}+{term}+A6")),
                ("A5", "{10,11,12,13;20,21,22,23}"),
            ]);
            let mut run = Evaluation::with_workbook(&book)
                .with_extensions(&registry)
                .build(Default::default())
                .unwrap();
            if unused || cacheability == ExtensionCacheability::Deterministic {
                assert_eq!(run.cell(target("D5")).unwrap(), 5.0.into());
                assert_eq!(calls.load(Ordering::SeqCst), if unused { 0 } else { 2 });
            } else {
                assert!(
                    matches!(run.cell(target("D5")),Err(EvaluationError::UnsupportedCapability {reason,..}) if matches!(reason, crate::eval::limits::EvaluationReasonError::SpillSettlementWouldReplayAnActuallyConsumedOpaqueOrVolatileProvider))
                );
                assert_eq!(calls.load(Ordering::SeqCst), 1);
                assert_eq!(run.values().count(), 0);
            }
        }
    }
}

#[test]
fn snapshots_and_editing_the_winner_preserve_and_then_recompute_priority() {
    let book = workbook(&[("B1", "{10,11;12,13}"), ("A2", "{1,2;3,4}")]);
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("A3"))
            .unwrap(),
        CellValue::Blank
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&baseline).unwrap();
    {
        let mut run = restored.evaluation(&context).unwrap();
        assert_eq!(run.cell(target("C2")).unwrap(), 13.0.into());
        assert_eq!(
            run.cell(target("A2")).unwrap(),
            CellValue::Error(CellError::Spill)
        );
        assert_eq!(run.usage().formula_executions, 0);
    }
    restored.set_formula(target("B1"), "{10,11}").unwrap();
    assert!(restored.cached_value(target("A2")).is_none());
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(target("A3")).unwrap(), 3.0.into());
    assert_eq!(run.cell(target("B3")).unwrap(), 4.0.into());
}

#[test]
fn small_rectangles_match_a_direct_coordinate_priority_oracle() {
    for height in 1..=3 {
        for width in 1..=2 {
            for lower_height in 1..=2 {
                for lower_width in 1..=3 {
                    let literal = |rows, cols, value: &str| {
                        "{".to_owned()
                            + &std::iter::repeat_n(
                                std::iter::repeat_n(value, cols)
                                    .collect::<Vec<_>>()
                                    .join(","),
                                rows,
                            )
                            .collect::<Vec<_>>()
                            .join(";")
                            + "}"
                    };
                    let book = workbook(&[
                        ("B1", &literal(height, width, "10")),
                        ("A2", &literal(lower_height, lower_width, "20")),
                    ]);
                    let upper = |r, c| r < height && c >= 1 && c < 1 + width;
                    let lower = |r, c| r >= 1 && r < 1 + lower_height && c < lower_width;
                    let overlap = (0..5).any(|r| (0..5).any(|c| upper(r, c) && lower(r, c)));
                    let mut expected = Vec::new();
                    for row in 0..5 {
                        for col in 0..5 {
                            let address = crate::model::CellAddress { row, col }.to_string();
                            let value = if upper(row as usize, col as usize) {
                                10.0.into()
                            } else if row == 1 && col == 0 && overlap {
                                CellValue::Error(CellError::Spill)
                            } else if lower(row as usize, col as usize) && !overlap {
                                20.0.into()
                            } else {
                                CellValue::Blank
                            };
                            expected.push((address, value));
                        }
                    }
                    let refs: Vec<_> = expected
                        .iter()
                        .map(|(a, v)| (a.as_str(), v.clone()))
                        .collect();
                    verify(&book, &refs);
                }
            }
        }
    }
}

#[test]
fn a_failed_output_batch_cannot_publish_its_earlier_successful_component() {
    let book = workbook(&[("A1", "{1,2}"), ("D1", "{3,4}"), ("J1", "{1,2}+K1")]);
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        assert_eq!(run.cell(target("B1")).unwrap(), 2.0.into());
        let before = run.result_changes().unwrap();
        assert!(run.outputs(&[target("E1"), target("K1")]).is_err());
        assert_eq!(run.result_changes().unwrap(), before);
    }
    assert_eq!(
        overlay.cached_value(target("B1")).as_deref(),
        Some(&CellValue::Number(2.0))
    );
    assert!(overlay.cached_value(target("D1")).is_none());
    assert!(overlay.cached_value(target("E1")).is_none());
}

#[test]
fn settlement_keeps_cumulative_work_and_fails_atomically_under_a_work_limit() {
    use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
    use crate::eval::limits::{EvaluationBudgets, EvaluationOutcome};
    use crate::eval::run::{EvaluationRun, WorkbookView};
    let book = workbook(&[
        ("D1", "{1;2;3;4;5}+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
    ]);
    let policy = PreparationPolicy::default();
    let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
    let mut aborted = 0;
    for limit in [64, 96, 128, 192, 256] {
        let mut run = EvaluationRun::new(
            WorkbookView::new(&book, &plan, &policy).unwrap(),
            Default::default(),
            EvaluationBudgets {
                run_work: limit,
                ..Default::default()
            },
        )
        .unwrap();
        let result = run.evaluate_cell(target("D5"));
        let report = run.finish(result.map(|_| ()));
        match report.outcome {
            EvaluationOutcome::Aborted {
                failure: EvaluationError::Budget(_),
                usage,
            } => {
                aborted += 1;
                assert!(usage.work_units <= limit);
            }
            EvaluationOutcome::Complete { .. } => {}
            other => panic!("unexpected failure {other:?}"),
        }
    }
    assert!(aborted > 0, "bounded work must include private attempts");
}

#[test]
fn replay_discovers_new_branch_dependencies_before_settling() {
    let book = workbook(&[
        ("D1", "{1;2;3;4;5}+IF(SUM(A6)=0,J2,0)+A6"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
        ("J1", "SEQUENCE(2)"),
    ]);
    verify(
        &book,
        &[
            ("D1", 3.0.into()),
            ("D5", 7.0.into()),
            ("A6", CellValue::Blank),
            ("J2", 2.0.into()),
        ],
    );
}

#[test]
fn repeated_losing_masks_with_a_larger_component_are_not_the_same_state() {
    let book = workbook(&[
        ("D1", "IF(SUM(A6)=0,{1;2}+J2,{1;2;3;4;5})"),
        ("A5", "{10,11,12,13;20,21,22,23}"),
        ("J1", "SEQUENCE(2)"),
    ]);
    let mut run = Evaluation::with_workbook(&book)
        .build(Default::default())
        .unwrap();
    assert!(
        matches!(run.cell(target("D1")),Err(EvaluationError::UnsupportedCapability {reason,..}) if matches!(reason, crate::eval::limits::EvaluationReasonError::SpillOwnershipComponentDoesNotSettle))
    );
    // Attempts execute 2, then 3, then 3, then 3 producers. The initial
    // two-producer states do not prove periodicity of the enlarged component.
    assert_eq!(run.usage().formula_executions, 11);
}

#[test]
fn an_outer_scalar_keeps_earlier_point_and_range_dependencies_across_settlement() {
    for prefix in ["G10", "SUM(G10:G11)"] {
        let mut book = workbook(&[
            ("D1", "{1;2;3;4;5}+A6"),
            ("A5", "{10,11,12,13;20,21,22,23}"),
            ("J10", &format!("{prefix}+SUM(D1:D5)")),
        ]);
        book.sheet_mut(0)
            .unwrap()
            .set_value(target("G10").address, 10.0)
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book).build().unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("J10"))
                .unwrap(),
            25.0.into()
        );
        overlay.set_value(target("G10"), 20.0).unwrap();
        assert!(overlay.cached_value(target("J10")).is_none());
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("J10"))
                .unwrap(),
            35.0.into()
        );
        overlay.set_value(target("G11"), 5.0).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("J10"))
                .unwrap(),
            if prefix.starts_with("SUM") {
                40.0.into()
            } else {
                35.0.into()
            }
        );
    }
}
