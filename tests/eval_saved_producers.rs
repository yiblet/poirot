//! Original public tests for Poirot's caller-trusted saved-result contract.
//! Array layout uses row-major literals; expected saved numbers are supplied explicitly,
//! rather than inferred from evaluation. No upstream tests or fixtures were reused.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::{EvaluationContext, EvaluationPolicy};
use poirot::eval::{EvaluationOutcome, EvaluationTarget, ProducerPolicy, RunConfiguration};
use poirot::model::{CellError, CellValue, Workbook};

fn at(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn policy() -> EvaluationPolicy {
    EvaluationPolicy {
        producers: ProducerPolicy::TrustSaved,
        ..Default::default()
    }
}
fn saved() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "{1,2;3,4}").unwrap();
    sheet.get_mut(at("A1").address).unwrap().value = 10.0.into();
    sheet
        .set_spill_extent(at("A1").address, at("B2").address, "{1,2;3,4}")
        .unwrap();
    for (address, value) in [("B1", 20.0), ("A2", 30.0), ("B2", 40.0)] {
        sheet
            .set_spill_cached_value(
                at(address).address,
                at("A1").address,
                "{1,2;3,4}",
                value.into(),
            )
            .unwrap();
    }
    sheet.set_formula(at("D1").address, "SUM(A1#)").unwrap();
    sheet.set_formula(at("D2").address, "ROWS(A1#)").unwrap();
    book
}
#[test]
fn default_computes_and_trust_restores_complete_saved_unit_and_scalar_consumers() {
    let book = saved();
    for (p, sum, child) in [
        (EvaluationPolicy::default(), 10.0, 4.0),
        (policy(), 100.0, 40.0),
    ] {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(p.clone())
            .build()
            .unwrap();
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(p)
            .build(Default::default())
            .unwrap();
        for _ in 0..2 {
            let context = EvaluationContext::default();
            let mut eval = overlay.evaluation(&context).unwrap();
            assert_eq!(eval.cell(at("B2")).unwrap(), child.into());
            assert_eq!(eval.cell(at("D1")).unwrap(), sum.into());
            assert_eq!(eval.cell(at("D2")).unwrap(), 2.0.into());
            assert_eq!(direct.cell(at("B2")).unwrap(), child.into());
            assert_eq!(direct.cell(at("D1")).unwrap(), sum.into());
        }
    }
}
#[test]
fn missing_or_explicit_blank_scalar_is_honest_and_producer_is_never_executed() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula(at("A1").address, "IF(TRUE,7,{1,2})")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert_eq!(direct.cell(at("A1")).unwrap(), CellValue::Blank);
    assert_eq!(direct.usage().formula_executions, 0);
    let configuration =
        RunConfiguration::default().with_producer_policy(ProducerPolicy::TrustSaved);
    assert_ne!(
        configuration.fingerprint().computation(),
        RunConfiguration::default().fingerprint().computation()
    );
    assert!(
        matches!(poirot::eval::run_cell_with_configuration(&book, 0, at("A1").address, &configuration, Default::default()), EvaluationOutcome::Complete { value, .. } if value == CellValue::Blank)
    );
}
#[test]
fn declared_incomplete_and_legacy_undeclared_arrays_fail_without_recalculation() {
    for declared in [false, true] {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_formula(at("A1").address, "{1,2;3,4}").unwrap();
        sheet
            .set_spill_cached_value(at("B1").address, at("A1").address, "{1,2;3,4}", 99.0.into())
            .unwrap();
        if declared {
            sheet
                .set_spill_extent(at("A1").address, at("B2").address, "{1,2;3,4}")
                .unwrap();
        }
        let mut direct = Evaluation::with_workbook(&book)
            .with_policy(policy())
            .build(Default::default())
            .unwrap();
        assert!(direct.cell(at("A1")).is_err());
        assert_eq!(direct.usage().formula_executions, 0);
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy())
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("A1"))
                .is_err()
        );
        assert!(overlay.changes().is_empty());
    }
}
#[test]
fn saved_error_member_preserves_geometry_and_singleton_identity() {
    let mut book = saved();
    book.sheet_mut(0)
        .unwrap()
        .get_mut(at("A1").address)
        .unwrap()
        .value = CellValue::Error(CellError::Div0);
    let mut eval = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert_eq!(eval.cell(at("D2")).unwrap(), 2.0.into());
    assert_eq!(
        eval.cell(at("D1")).unwrap(),
        CellValue::Error(CellError::Div0)
    );
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "{7}").unwrap();
    sheet
        .set_spill_extent(at("A1").address, at("A1").address, "{7}")
        .unwrap();
    sheet.set_formula(at("D1").address, "ROWS(A1#)").unwrap();
    let mut eval = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert_eq!(eval.cell(at("D1")).unwrap(), 1.0.into());
}
#[test]
fn content_overrides_release_baseline_children_and_child_blockers_fail() {
    let book = saved();
    for child_formula in [false, true] {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy())
            .build()
            .unwrap();
        if child_formula {
            overlay.set_formula(at("B2"), "42").unwrap();
        } else {
            overlay.set_value(at("B2"), 42.0).unwrap();
        }
        let context = EvaluationContext::default();
        assert!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("A1"))
                .is_err()
        );
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        40.0.into()
    );
    overlay.set_formula(at("A1"), "{8,9}").unwrap();
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("A1")).unwrap(), CellValue::Blank);
        assert_eq!(eval.cell(at("B2")).unwrap(), CellValue::Blank);
    }
    overlay.set_value(at("A1"), 55.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        CellValue::Blank
    );
}
#[test]
fn style_only_edits_preserve_saved_contents_and_policy_switch_recomputes() {
    let book = saved();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        100.0.into()
    );
    let style = poirot::model::CellStyle {
        number_format: Some("0.00".into()),
        ..Default::default()
    };
    overlay.set_style(at("B2"), style).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        100.0.into()
    );
    overlay.set_policy(EvaluationPolicy::default()).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        10.0.into()
    );
}
#[test]
fn trusted_captured_clock_producer_stays_stale_but_scalar_clock_recomputes() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet
        .set_formula_with_cached_value(at("A1").address, "IF(TRUE,NOW(),{1,2})", 11.0.into())
        .unwrap();
    sheet.set_formula(at("D1").address, "NOW()").unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let mut context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        ..Default::default()
    };
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("A1")).unwrap(), 11.0.into());
        assert_eq!(eval.cell(at("D1")).unwrap(), 25569.0.into());
        assert_eq!(eval.usage().formula_executions, 1);
    }
    context.as_of_time += std::time::Duration::from_secs(86400);
    let mut eval = overlay.evaluation(&context).unwrap();
    assert_eq!(eval.cell(at("A1")).unwrap(), 11.0.into());
    assert_eq!(eval.cell(at("D1")).unwrap(), 25570.0.into());
    assert_eq!(eval.usage().formula_executions, 1);
}
#[test]
fn snapshot_preserves_policy_complete_saved_values_and_precise_live_retention() {
    for p in [policy(), EvaluationPolicy::default()] {
        let book = saved();
        let expected = if p.producers == ProducerPolicy::TrustSaved {
            100.0
        } else {
            10.0
        };
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(p.clone())
            .build()
            .unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .unwrap(),
            expected.into()
        );
        let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
        let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
        let mut restored = state.attach(&book).unwrap();
        assert_eq!(restored.policy(), &p);
        let mut eval = restored.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("D1")).unwrap(), expected.into());
        assert_eq!(
            eval.cell(at("B2")).unwrap(),
            if expected == 100.0 {
                40.0.into()
            } else {
                4.0.into()
            }
        );
        assert_eq!(eval.usage().formula_executions, 0);
    }
}
#[test]
fn live_retained_footprint_shrinks_releases_children_and_survives_context_changes() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value(at("G1").address, true).unwrap();
    sheet
        .set_formula(at("A1").address, "IF(G1,{1,2;3,4},7)")
        .unwrap();
    sheet.set_formula(at("D1").address, "SUM(A1:B2)").unwrap();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        10.0.into()
    );
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("B2")).unwrap(), 4.0.into());
        assert_eq!(eval.usage().formula_executions, 0);
    }
    let mut changed_context = context.clone();
    changed_context.as_of_time += std::time::Duration::from_secs(3600);
    {
        let mut eval = overlay.evaluation(&changed_context).unwrap();
        assert_eq!(eval.cell(at("D1")).unwrap(), 10.0.into());
        assert_eq!(eval.usage().formula_executions, 0);
    }
    overlay.set_value(at("G1"), false).unwrap();
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("B2")).unwrap(), CellValue::Blank);
        assert_eq!(eval.cell(at("D1")).unwrap(), 7.0.into());
    }
    overlay.remove_cell(at("A1")).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        0.0.into()
    );
}
#[test]
fn xlsx_roundtrip_trusts_saved_values_and_source_snapshot_keeps_incompleteness() {
    let book = saved();
    let bytes = poirot_io::write_xlsx_to_buffer(&book).unwrap();
    let reopened = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(
        reopened
            .sheet(0)
            .unwrap()
            .complete_spill_extent(at("A1").address),
        Some(at("B2").address)
    );
    let mut eval = Evaluation::with_workbook(&reopened)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert_eq!(eval.cell(at("D1")).unwrap(), 100.0.into());
    let mut incomplete = Workbook::new();
    let sheet = incomplete.add_sheet("S").unwrap();
    sheet.set_formula(at("A1").address, "{1,2}").unwrap();
    sheet
        .set_spill_extent(at("A1").address, at("B1").address, "{1,2}")
        .unwrap();
    let snapshot = poirot::model::snapshot::Snapshot::capture(&incomplete);
    let incomplete = snapshot.restore().unwrap();
    let mut eval = Evaluation::with_workbook(&incomplete)
        .with_policy(policy())
        .build(Default::default())
        .unwrap();
    assert!(eval.cell(at("A1")).is_err());
}
#[test]
fn successful_trusted_read_then_child_edit_cannot_bypass_saved_validation() {
    let book = saved();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        2.0.into()
    );
    overlay.set_value(at("B2"), 99.0).unwrap();
    assert!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .is_err()
    );
}
#[test]
fn style_reset_roundtrip_recovery_and_flush_preserve_saved_ownership() {
    let mut book = saved();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&mut book)
        .with_policy(policy())
        .build()
        .unwrap();
    for address in ["A1", "B2"] {
        overlay
            .set_style(
                at(address),
                poirot::model::CellStyle {
                    number_format: Some("0.00".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        overlay.set_style(at(address), Default::default()).unwrap();
    }
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        100.0.into()
    );
    let (baseline, state) = Overlay::from_snapshot(
        postcard::from_bytes(&postcard::to_allocvec(&overlay.snapshot()).unwrap()).unwrap(),
    )
    .unwrap();
    let recovered = state.source_workbook(&baseline).unwrap();
    assert_eq!(
        recovered
            .sheet(0)
            .unwrap()
            .complete_spill_extent(at("A1").address),
        Some(at("B2").address)
    );
    let mut restored = state.attach(&baseline).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        100.0.into()
    );
    // Resetting the anchor style removes its redundant override; the absent child
    // keeps one explicit authored blank/style edit.
    assert_eq!(overlay.flush().unwrap(), 1);
    assert_eq!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .complete_spill_extent(at("A1").address),
        Some(at("B2").address)
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        100.0.into()
    );
}
#[test]
fn explicit_blank_content_dominates_style_edits_in_either_order() {
    let book = saved();
    let context = EvaluationContext::default();
    for style_first in [true, false] {
        let mut overlay = Overlay::with_workbook(&book)
            .with_policy(policy())
            .build()
            .unwrap();
        let style = poirot::model::CellStyle {
            number_format: Some("0.00".into()),
            ..Default::default()
        };
        if style_first {
            overlay.set_style(at("B2"), style).unwrap();
            overlay.set_value(at("B2"), CellValue::Blank).unwrap();
        } else {
            overlay.set_value(at("B2"), CellValue::Blank).unwrap();
            overlay.set_style(at("B2"), style).unwrap();
        }
        assert!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .is_err()
        );
        let (baseline, state) = Overlay::from_snapshot(
            postcard::from_bytes(&postcard::to_allocvec(&overlay.snapshot()).unwrap()).unwrap(),
        )
        .unwrap();
        let mut restored = state.attach(&baseline).unwrap();
        assert!(
            restored
                .evaluation(&context)
                .unwrap()
                .cell(at("D1"))
                .is_err()
        );
    }
}
/// Record owned completion summaries without retaining borrowed values.
#[derive(Default)]
struct Observations(Vec<(EvaluationTarget, poirot::eval::CellProvenance, bool)>);
impl poirot::eval::CellObserver for Observations {
    fn completed(
        &mut self,
        cell: poirot::eval::CellCompletion<'_>,
    ) -> Result<(), poirot::eval::EvaluationFailure> {
        self.0
            .push((cell.target, cell.provenance, cell.result.is_ok()));
        Ok(())
    }
}
#[test]
fn trusted_producer_observers_report_external_origin_on_success_and_failure() {
    for incomplete in [false, true] {
        let mut book = saved();
        if incomplete {
            book.sheet_mut(0).unwrap().remove(at("B2").address);
            book.sheet_mut(0)
                .unwrap()
                .set_spill_extent(at("A1").address, at("B2").address, "{1,2;3,4}")
                .unwrap();
        }
        let prepare = poirot::eval::PreparationPolicy::default();
        let plan = poirot::eval::FormulaCompiler::new().compile_workbook(&book, &prepare);
        let view = poirot::eval::WorkbookView::new(&book, &plan, &prepare).unwrap();
        let config = RunConfiguration::default().with_producer_policy(ProducerPolicy::TrustSaved);
        let mut observations = Observations::default();
        let mut run =
            poirot::eval::EvaluationRun::new_with_configuration(view, &config, Default::default())
                .unwrap()
                .with_cell_observer(&mut observations);
        assert_eq!(run.evaluate_cell(at("A1")).is_err(), incomplete);
        assert_eq!(run.usage().formula_executions, 0);
        drop(run);
        assert!(observations.0.contains(&(
            at("A1"),
            poirot::eval::CellProvenance::ExternalCache,
            !incomplete
        )));
    }
}
#[test]
fn scalar_intersection_ignores_trust_and_changed_anchor_uses_its_own_cache() {
    let book = saved();
    let mut scalar = policy();
    scalar.arrays = poirot::eval::ArrayResultPolicy::ScalarIntersection;
    let mut eval = Evaluation::with_workbook(&book)
        .with_policy(scalar)
        .build(Default::default())
        .unwrap();
    assert_eq!(eval.cell(at("A1")).unwrap(), 1.0.into());
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    overlay.set_formula(at("A1"), "{7,8}").unwrap();
    overlay.set_cached_value(at("A1"), 70.0).unwrap();
    let context = EvaluationContext::default();
    let mut eval = overlay.evaluation(&context).unwrap();
    assert_eq!(eval.cell(at("A1")).unwrap(), 70.0.into());
    assert_eq!(eval.cell(at("B1")).unwrap(), CellValue::Blank);
    assert_eq!(
        eval.cell(at("D2")).unwrap(),
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(eval.usage().formula_executions, 1);
}
#[test]
fn explicit_shared_inputs_at_anchor_or_child_never_restore_old_children() {
    let book = saved();
    let plan = poirot::eval::FormulaCompiler::new().compile_workbook(&book, &Default::default());
    let shared = poirot::eval::SharedWorkbook::new(book, std::sync::Arc::new(plan)).unwrap();
    let configuration =
        RunConfiguration::default().with_producer_policy(ProducerPolicy::TrustSaved);
    for (address, input, aborts) in [
        ("A1", CellValue::Number(5.0), false),
        ("B2", CellValue::Blank, true),
    ] {
        let (_, inputs) = shared
            .prepare_inputs(|_| {
                Ok::<_, poirot::model::ModelError>(((), vec![(0, at(address).address, input)], 1))
            })
            .unwrap();
        let result = shared
            .run_workbook(&configuration, Default::default(), &inputs, |run, _| {
                run.evaluate_cell(at("B2"))
            })
            .unwrap();
        match result.report.outcome {
            EvaluationOutcome::Complete { value, .. } => {
                assert!(!aborts);
                assert_eq!(value, CellValue::Blank);
            }
            EvaluationOutcome::Aborted { .. } => assert!(aborts),
        }
        assert_eq!(result.report.execution.formula_executions, 0);
    }
}
#[test]
fn byte_equal_blank_content_after_style_flush_declines_old_saved_ownership() {
    let mut book = saved();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&mut book)
        .with_policy(policy())
        .build()
        .unwrap();
    overlay
        .set_style(
            at("B2"),
            poirot::model::CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    overlay.flush().unwrap();
    assert_eq!(
        overlay.source_cell(at("B2")).unwrap().value,
        CellValue::Blank
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        40.0.into()
    );
    overlay.set_value(at("B2"), CellValue::Blank).unwrap();
    assert!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .is_err()
    );
    overlay.set_policy(EvaluationPolicy::default()).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        4.0.into()
    );
}
#[test]
fn lookup_consumers_follow_saved_membership_and_policy_changes() {
    let mut book = saved();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_formula(at("G1").address, "MATCH(40,A2:B2,0)")
        .unwrap();
    sheet
        .set_formula(at("G2").address, "SUM(XLOOKUP(40,A2:B2,A1:B1))")
        .unwrap();
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    for _ in 0..2 {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("G1")).unwrap(), 2.0.into());
        assert_eq!(eval.cell(at("G2")).unwrap(), 20.0.into());
    }
    overlay.set_policy(EvaluationPolicy::default()).unwrap();
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(
            eval.cell(at("G1")).unwrap(),
            CellValue::Error(CellError::NA)
        );
        assert_eq!(
            eval.cell(at("G2")).unwrap(),
            CellValue::Error(CellError::NA)
        );
    }
    overlay.set_policy(policy()).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G2"))
            .unwrap(),
        20.0.into()
    );
}

#[test]
fn trusted_scalar_text_is_admitted_before_copying_and_aborts_without_publication() {
    let mut book = Workbook::new();
    book.add_sheet("S")
        .unwrap()
        .set_formula_with_cached_value(
            at("A1").address,
            "IF(TRUE,7,{1,2})",
            CellValue::Text("x".repeat(65_536)),
        )
        .unwrap();
    let context = EvaluationContext {
        limits: poirot::eval::EvaluationLimits::default().max_text_bytes(8),
        ..Default::default()
    };
    let expected = poirot::eval::EvaluationFailure::Budget(poirot::eval::BudgetFailure {
        kind: poirot::eval::BudgetKind::TextBytes,
        limit: 8,
        consumed: 0,
        requested: 65_536,
        target: Some(at("A1")),
    });
    let mut direct = Evaluation::with_workbook(&book)
        .with_policy(policy())
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("A1")).unwrap_err(), expected);
    assert_eq!(direct.usage().formula_executions, 0);
    assert!(direct.into_changes().is_empty());
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(policy())
        .build()
        .unwrap();
    {
        let mut eval = overlay.evaluation(&context).unwrap();
        assert_eq!(eval.cell(at("A1")).unwrap_err(), expected);
        assert_eq!(eval.usage().formula_executions, 0);
    }
    assert!(overlay.changes().is_empty());
}
