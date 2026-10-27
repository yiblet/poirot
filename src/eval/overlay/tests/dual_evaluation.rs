//! Behavioral parity of editable direct workspaces and persistent overlays.
use crate::eval::config::ArrayResultPolicy;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::overlay::evaluation::Evaluation;
use crate::eval::overlay::incremental::Overlay;
use crate::model::{CellValue, Workbook};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("S").unwrap();
    sheet.set_value(at("A1").address, 2.).unwrap();
    for (cell, formula) in [
        ("B1", "A1*2"),
        ("C1", "B1+1"),
        ("D1", "7*3"),
        ("E1", "SUM(A2:A8)"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    workbook
}

fn context() -> EvaluationContext {
    let context = EvaluationContext::default();
    let localization = context.localization;
    context.with_clock(
        UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        42,
        localization,
    )
}

#[test]
fn direct_edits_clear_results_but_preserve_programs_and_work_counters() {
    let book = workbook();
    let mut direct = Evaluation::with_workbook(&book).build(context()).unwrap();
    assert!(std::ptr::eq(direct.workbook(), &book));
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(5.));
    assert_eq!(direct.cell(at("D1")).unwrap(), CellValue::Number(21.));
    let parsed = direct.compiled_usage().parse_attempts;
    let executed = direct.usage().formula_executions;
    direct.set_value(at("A1"), 8.).unwrap();
    assert_eq!(direct.cached_cells(), 0);
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(17.));
    direct.set_value(at("A1"), 10.).unwrap();
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(21.));
    assert!(direct.usage().formula_executions > executed);
    assert_eq!(direct.compiled_usage().parse_attempts, parsed);
    direct.set_value(at("A1"), 2.).unwrap();
    assert_eq!(direct.edited_cells(), 0);
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(5.));
    direct.begin(context()).unwrap();
    assert_eq!(direct.usage().formula_executions, 0);
    assert_eq!(direct.cached_cells(), 0);
    assert_eq!(direct.compiled_usage().parse_attempts, parsed);
}

#[test]
fn direct_and_incremental_paths_match_across_sparse_edits_and_output_modes() {
    let book = workbook();
    let context = context();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let outputs = [at("C1"), at("E1"), at("D1"), at("C1")];
    for n in 0..24 {
        direct.set_value(at("A1"), n as f64).unwrap();
        overlay.set_value(at("A1"), n as f64).unwrap();
        if n % 2 == 0 {
            direct.set_value(at("A5"), n as f64).unwrap();
            overlay.set_value(at("A5"), n as f64).unwrap();
        } else {
            direct.remove_cell(at("A5")).unwrap();
            overlay.remove_cell(at("A5")).unwrap();
        }
        let a = direct.outputs(&outputs).unwrap();
        let b = overlay
            .evaluation(&context)
            .unwrap()
            .outputs(&outputs)
            .unwrap();
        assert_eq!(a, b);
    }
    direct.set_formula(at("B1"), "A1*3").unwrap();
    overlay.set_formula(at("B1"), "A1*3").unwrap();
    assert_eq!(
        direct.all().unwrap(),
        overlay.evaluation(&context).unwrap().all().unwrap()
    );
    let executions = overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("D1"))
        .unwrap();
    assert_eq!(executions, CellValue::Number(21.));
    direct.reset();
    overlay.reset();
    assert_eq!(
        direct.all().unwrap(),
        overlay.evaluation(&context).unwrap().all().unwrap()
    );
}

#[test]
fn workspaces_are_send_with_borrowed_and_arc_baselines_and_used_state() {
    fn assert_send<T: Send>() {}
    assert_send::<Overlay<'_, &Workbook>>();
    assert_send::<Overlay<'static, Arc<Workbook>>>();
    assert_send::<Evaluation<'_, &Workbook>>();
    assert_send::<Evaluation<'static, Arc<Workbook>>>();
    let book = Arc::new(workbook());
    let mut overlay = Overlay::with_workbook(Arc::clone(&book)).build().unwrap();
    overlay
        .evaluation(&context())
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    let overlay = std::thread::spawn(move || {
        overlay.set_value(at("A1"), 6.).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context())
                .unwrap()
                .cell(at("C1"))
                .unwrap(),
            CellValue::Number(13.)
        );
        overlay
    })
    .join()
    .unwrap();
    let mut direct = Evaluation::with_workbook(Arc::clone(&book))
        .build(context())
        .unwrap();
    direct.cell(at("C1")).unwrap();
    std::thread::spawn(move || {
        direct.set_value(at("A1"), 7.).unwrap();
        assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(15.));
    })
    .join()
    .unwrap();
    std::thread::scope(|scope| {
        let mut borrowed = Overlay::with_workbook(book.as_ref()).build().unwrap();
        scope.spawn(move || {
            assert_eq!(
                borrowed
                    .evaluation(&context())
                    .unwrap()
                    .cell(at("C1"))
                    .unwrap(),
                CellValue::Number(5.)
            )
        });
    });
    assert_eq!(overlay.edited_cells(), 1);
}

#[test]
fn snapshots_restore_to_chosen_handles_and_reject_changed_baselines() {
    let book = workbook();
    let context = context();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    overlay.set_value(at("A1"), 8.).unwrap();
    overlay
        .evaluation(&context)
        .unwrap()
        .cell(at("C1"))
        .unwrap();
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (baseline, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(Arc::new(baseline)).unwrap();
    let mut run = restored.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("C1")).unwrap(), CellValue::Number(17.));
    assert_eq!(run.usage().formula_executions, 0);
    let (mut baseline, state) =
        Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    baseline
        .sheet_mut(0)
        .unwrap()
        .set_value(at("A1").address, 999.)
        .unwrap();
    assert!(state.attach(&baseline).is_err());
}

#[test]
fn direct_snapshots_reuse_bytecode_without_restoring_untracked_results() {
    let book = workbook();
    let mut direct = Evaluation::with_workbook(&book).build(context()).unwrap();
    direct.set_value(at("A1"), 9.).unwrap();
    direct.cell(at("C1")).unwrap();
    let bytes = postcard::to_allocvec(&direct.snapshot()).unwrap();
    let (baseline, state) =
        Evaluation::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.into_evaluation(&baseline, context()).unwrap();
    assert_eq!(restored.cached_cells(), 0);
    assert_eq!(restored.cell(at("C1")).unwrap(), CellValue::Number(19.));
    assert_eq!(restored.compiled_usage().parse_attempts, 0);
    assert_eq!(restored.usage().formula_executions, 2);
    let changes: Vec<crate::model::WorkbookChange> = restored.into_changes();
    let mut applied = baseline.clone();
    applied.apply(changes).unwrap();
    assert_eq!(
        applied
            .sheet(0)
            .unwrap()
            .get(at("C1").address)
            .unwrap()
            .value,
        CellValue::Number(19.)
    );
    assert!(
        applied
            .sheet(0)
            .unwrap()
            .get(at("C1").address)
            .unwrap()
            .formula
            .is_some()
    );
}

#[test]
fn spills_intersections_dynamic_references_and_randomness_share_one_engine() {
    let mut book = workbook();
    let sheet = book.sheet_mut(0).unwrap();
    for (cell, formula) in [
        ("G1", "IF(A1>2,{1,2;3,4;5,6;7,8},{1,2})"),
        ("K1", "SUM(G1:H8)"),
        ("L1", "@G1:G2"),
        ("M1", "INDIRECT(\"B1\")"),
        ("N1", "RAND()"),
        ("O1", "RAND()+RAND()"),
        ("P2", "{10,20;30,40}"),
        ("Q1", "{50;60;70}"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let policy = EvaluationPolicy {
        arrays: ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    };
    let context = context();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    for height in [2., 4., 1., 3.] {
        direct.set_value(at("A1"), height).unwrap();
        overlay.set_value(at("A1"), height).unwrap();
        let outputs = [
            at("H2"),
            at("K1"),
            at("L1"),
            at("M1"),
            at("O1"),
            at("N1"),
            at("Q2"),
            at("P2"),
        ];
        let actual = direct.outputs(&outputs).unwrap();
        assert_eq!(
            actual[0].value,
            if height > 2. {
                CellValue::Number(4.)
            } else {
                CellValue::Blank
            }
        );
        assert_eq!(
            actual[1].value,
            CellValue::Number(if height > 2. { 36. } else { 3. })
        );
        assert_eq!(actual[2].value, CellValue::Number(1.));
        assert_eq!(actual[3].value, CellValue::Number(height * 2.));
        assert!(matches!(actual[4].value,CellValue::Number(value) if (0. ..2.).contains(&value)));
        assert!(matches!(actual[5].value,CellValue::Number(value) if (0. ..1.).contains(&value)));
        assert_eq!(actual[6].value, CellValue::Number(60.));
        assert_eq!(
            actual[7].value,
            CellValue::Error(crate::model::CellError::Spill)
        );
        assert_eq!(
            actual,
            overlay
                .evaluation(&context)
                .unwrap()
                .outputs(&outputs)
                .unwrap()
        );
    }
    direct.set_value(at("H2"), 99.).unwrap();
    overlay.set_value(at("H2"), 99.).unwrap();
    assert_eq!(
        direct.cell(at("G1")).unwrap(),
        CellValue::Error(crate::model::CellError::Spill)
    );
    assert_eq!(direct.cell(at("H2")).unwrap(), CellValue::Number(99.));
    assert_eq!(
        direct.all().unwrap(),
        overlay.evaluation(&context).unwrap().all().unwrap()
    );
}

#[test]
fn failure_poisoning_and_new_context_are_explicit() {
    let book = workbook();
    let mut limited = context();
    limited.limits = limited.limits.max_work(1);
    let mut direct = Evaluation::with_workbook(&book).build(limited).unwrap();
    assert!(direct.outputs(&[at("C1"), at("D1")]).is_err());
    assert_eq!(direct.cached_cells(), 0);
    assert!(direct.changes().is_empty());
    direct.set_value(at("A1"), 7.).unwrap();
    assert!(direct.cell(at("C1")).is_err());
    direct.begin(context()).unwrap();
    assert_eq!(direct.cell(at("C1")).unwrap(), CellValue::Number(15.));
}

#[test]
fn failed_direct_batch_does_not_export_partial_results_or_erase_previous_success() {
    let book = workbook();
    let mut context = context();
    context.retention.cells = 1;
    let mut direct = Evaluation::with_workbook(&book).build(context).unwrap();
    assert_eq!(direct.cell(at("D1")).unwrap(), CellValue::Number(21.));
    let previous = direct.changes();
    assert!(!previous.is_empty());
    assert!(direct.outputs(&[at("C1"), at("E1")]).is_err());
    assert_eq!(direct.changes(), previous);
    assert_eq!(direct.cached_cells(), 1);
}

#[test]
fn baseline_attachment_preserves_exact_nan_and_signed_zero_identity() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_value(at("A3").address, f64::from_bits(0x7ff8000000000042))
        .unwrap();
    book.sheet_mut(0)
        .unwrap()
        .set_value(at("A4").address, -0.)
        .unwrap();
    let direct = Evaluation::with_workbook(&book).build(context()).unwrap();
    let bytes = postcard::to_allocvec(&direct.snapshot()).unwrap();
    let (baseline, state) =
        Evaluation::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    assert!(state.into_evaluation(&baseline, context()).is_ok());
    let (mut baseline, state) =
        Evaluation::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    baseline
        .sheet_mut(0)
        .unwrap()
        .set_value(at("A4").address, 0.)
        .unwrap();
    assert!(state.into_evaluation(&baseline, context()).is_err());
}

#[test]
fn all_values_exclude_implicit_blank_dependencies_regardless_of_cache_warmth() {
    let book = workbook();
    let context = context();
    let mut direct = Evaluation::with_workbook(&book)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let expected = direct.all().unwrap();
    assert!(!expected.iter().any(|cell| cell.target == at("A5")));
    assert_eq!(direct.cell(at("Z99")).unwrap(), CellValue::Blank);
    assert_eq!(direct.all().unwrap(), expected);
    for _ in 0..2 {
        let mut evaluation = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(at("Z99")).unwrap(), CellValue::Blank);
        assert_eq!(evaluation.all().unwrap(), expected);
    }
}

#[test]
fn all_values_drop_saved_spill_only_cells_after_a_producer_shrinks() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("G1").address, "IF(A1>1,{1,2;3,4},{1,2})")
        .unwrap();
    let policy = EvaluationPolicy {
        arrays: ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    };
    let context = context();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy.clone())
            .build()
            .unwrap();
        overlay.evaluation(&context).unwrap().all().unwrap();
        overlay.changes()
    };
    book.apply(changes).unwrap();
    assert!(
        book.sheet(0)
            .unwrap()
            .spill_owner(at("H2").address)
            .is_some()
    );
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy.clone())
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy)
        .build()
        .unwrap();
    direct.set_value(at("A1"), 1.).unwrap();
    overlay.set_value(at("A1"), 1.).unwrap();
    let expected = direct.all().unwrap();
    assert!(!expected.iter().any(|cell| cell.target == at("H2")));
    assert!(expected.iter().any(|cell| cell.target == at("H1")));
    assert_eq!(
        overlay.evaluation(&context).unwrap().all().unwrap(),
        expected
    );
}

#[test]
fn scalar_writeback_clears_saved_spill_children_on_both_paths() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("G1").address, "{1,2;3,4}")
        .unwrap();
    let context = context();
    let dynamic = EvaluationPolicy {
        arrays: ArrayResultPolicy::DynamicSpill,
        ..Default::default()
    };
    let changes = {
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(dynamic.clone())
            .build(context.clone())
            .unwrap();
        assert_eq!(direct.cell(at("H2")).unwrap(), CellValue::Number(4.));
        direct.changes()
    };
    book.apply(changes).unwrap();
    for switch_policy in [false, true] {
        let policy = if switch_policy {
            dynamic.clone()
        } else {
            EvaluationPolicy {
                arrays: ArrayResultPolicy::ScalarIntersection,
                ..Default::default()
            }
        };
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy.clone())
            .build(context.clone())
            .unwrap();
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy)
            .build()
            .unwrap();
        if switch_policy {
            assert_eq!(direct.cell(at("H2")).unwrap(), CellValue::Number(4.));
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("H2"))
                .unwrap();
            direct
                .set_policy(EvaluationPolicy {
                    arrays: ArrayResultPolicy::ScalarIntersection,
                    ..Default::default()
                })
                .unwrap();
            overlay
                .set_policy(EvaluationPolicy {
                    arrays: ArrayResultPolicy::ScalarIntersection,
                    ..Default::default()
                })
                .unwrap();
        }
        assert_eq!(direct.cell(at("G1")).unwrap(), CellValue::Number(1.));
        let mut evaluated = overlay.evaluation(&context).unwrap();
        assert_eq!(evaluated.cell(at("G1")).unwrap(), CellValue::Number(1.));
        let evaluated_changes = evaluated.result_changes().unwrap();
        drop(evaluated);
        for changes in [direct.changes(), evaluated_changes, overlay.changes()] {
            let mut applied = book.clone();
            applied.apply(changes).unwrap();
            assert_eq!(
                applied.sheet(0).unwrap().spill_owner(at("H2").address),
                None
            );
            assert!(applied.sheet(0).unwrap().get(at("H2").address).is_none());
            assert!(
                applied
                    .sheet(0)
                    .unwrap()
                    .get(at("G1").address)
                    .unwrap()
                    .formula
                    .is_some()
            );
        }
    }
}

#[test]
fn direct_publication_matches_batch_after_dependencies_spills_cached_calls_and_edits() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(at("G1").address, "{1,2;3,4}")
        .unwrap();
    let mut cells = Evaluation::with_workbook(&book).build(context()).unwrap();
    let mut batch = Evaluation::with_workbook(&book).build(context()).unwrap();
    let outputs = [at("C1"), at("D1"), at("G1"), at("H2"), at("C1")];
    for value in [2., 7., 11.] {
        cells.begin(context()).unwrap();
        batch.begin(context()).unwrap();
        cells.set_value(at("A1"), value).unwrap();
        batch.set_value(at("A1"), value).unwrap();
        let expected = [2. * value + 1., 21., 1., 4., 2. * value + 1.];
        for (target, value) in outputs.iter().zip(expected) {
            assert_eq!(cells.cell(*target).unwrap(), CellValue::Number(value));
        }
        batch.outputs(&outputs).unwrap();
        assert!(cells.cached_cells() >= 8);
        assert_eq!(cells.cached_value(at("H2")), Some(&CellValue::Number(4.)));
        assert!(!cells.changes().is_empty());
        assert_eq!(cells.changes(), batch.changes());
        assert_eq!(cells.cached_cells(), batch.cached_cells());
        let changes = cells.changes();
        let executed = cells.usage().formula_executions;
        cells.outputs(&outputs).unwrap();
        assert_eq!(cells.changes(), changes);
        assert_eq!(cells.usage().formula_executions, executed);
        for target in outputs {
            assert_eq!(cells.cached_value(target), batch.cached_value(target));
        }
    }
}

#[test]
fn overlay_deferred_range_index_tracks_per_cell_publication_edits_and_replaced_ranges() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for row in 1..=6 {
        sheet
            .set_value(at(&format!("A{row}")).address, row as f64)
            .unwrap();
    }
    for (cell, formula) in [
        ("C1", "SUM(A1:A3)"),
        ("C2", "SUM(A4:A6)"),
        ("D1", "SUM(C1:C2)"),
        ("E1", "D1+1"),
    ] {
        sheet.set_formula(at(cell).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = context();
    {
        let mut run = overlay.evaluation(&context).unwrap();
        for (cell, value) in [("C1", 6.), ("C2", 15.), ("D1", 21.), ("E1", 22.)] {
            assert_eq!(run.cell(at(cell)).unwrap(), CellValue::Number(value));
        }
    }
    for (cell, value, expected) in [("A2", 12., 32.), ("A5", 15., 42.)] {
        overlay.set_value(at(cell), value).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("E1"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
    overlay.set_formula(at("C1"), "SUM(B1:B3)").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(26.)
    );
    overlay.set_value(at("B2"), 9.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("E1"))
            .unwrap(),
        CellValue::Number(35.)
    );
    overlay.set_value(at("A2"), 99.).unwrap();
    let mut run = overlay.evaluation(&context).unwrap();
    assert_eq!(run.cell(at("E1")).unwrap(), CellValue::Number(35.));
    let executions = run.usage().formula_executions;
    assert_eq!(run.cell(at("E1")).unwrap(), CellValue::Number(35.));
    assert_eq!(run.usage().formula_executions, executions);
}
