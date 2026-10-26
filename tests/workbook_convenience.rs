//! Public workflow contracts; arithmetic expectations follow multiplication and SEQUENCE semantics.
use poirot::calculation::{EvaluationContext, EvaluationPolicy};
use poirot::eval::artifact;
use poirot::{CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Model inputs").unwrap();
    sheet.set_value("A1".parse().unwrap(), 3.).unwrap();
    sheet.set_formula("B1".parse().unwrap(), "A1*2").unwrap();
    sheet.set_formula("C1".parse().unwrap(), "A1*3").unwrap();
    // Deliberately stale saved values demonstrate partial export's honest contract.
    sheet.get_mut("B1".parse().unwrap()).unwrap().value = CellValue::Number(900.);
    sheet.get_mut("C1".parse().unwrap()).unwrap().value = CellValue::Number(901.);
    book
}
fn saved(book: &Workbook, cell: &str) -> CellValue {
    book.stored_cell("Model inputs", cell.parse().unwrap())
        .unwrap()
        .value
        .clone()
}

#[test]
fn direct_defaults_typed_targets_blank_and_errors_remain_distinct() {
    let book = book();
    let mut evaluation = book.evaluation().build().unwrap();
    let output = evaluation.target("'Model inputs'!B1").unwrap();
    let blank = evaluation.target("'Model inputs'!X20").unwrap();
    assert!(evaluation.target("Absent!A1").is_err());
    assert_eq!(evaluation.cell(blank).unwrap(), CellValue::Blank);
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(6.));
    let captured = evaluation.context().as_of_time;
    assert_eq!(evaluation.context().as_of_time, captured);
    assert_eq!(saved(&book, "B1"), CellValue::Number(900.));
    assert!(book.sheet_by_name("Absent").is_none());
    assert!(
        book.sheet_by_name("Model inputs")
            .unwrap()
            .get(blank.address)
            .is_none()
    );
    evaluation.set_formula(output, "1/0").unwrap();
    assert!(matches!(
        evaluation.cell(output).unwrap(),
        CellValue::Error(_)
    ));
    assert!(evaluation.finish().execution.is_none());
}

#[test]
fn direct_export_matches_manual_path_partial_full_and_continued_use() {
    let book = book();
    let mut evaluation = book.evaluation().build().unwrap();
    let input = evaluation.target("'Model inputs'!A1").unwrap();
    let output = evaluation.target("'Model inputs'!B1").unwrap();
    evaluation.set_value(input, 5.).unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(10.));
    let count = evaluation.cached_cells();
    let mut manual = book.model().clone();
    manual.apply(evaluation.changes()).unwrap();
    let bytes = evaluation.save_xlsx().to_buffer().unwrap();
    let exported = Workbook::from_bytes(&bytes).unwrap();
    let expected = Workbook::from_model(manual)
        .save_xlsx()
        .to_buffer()
        .unwrap();
    assert_eq!(exported, Workbook::from_bytes(&expected).unwrap());
    assert_eq!(saved(&exported, "A1"), CellValue::Number(5.));
    assert_eq!(saved(&exported, "B1"), CellValue::Number(10.));
    assert_eq!(saved(&exported, "C1"), CellValue::Number(901.));
    assert_eq!(evaluation.cached_cells(), count);
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(10.));
    evaluation.all().unwrap();
    let full = Workbook::from_bytes(&evaluation.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&full, "C1"), CellValue::Number(15.));
    assert_eq!(saved(&book, "A1"), CellValue::Number(3.));
}

#[test]
fn overlay_export_keeps_edits_and_warm_results() {
    let book = book();
    let context = EvaluationContext::default();
    let mut overlay = book.overlay().build().unwrap();
    let input = overlay.target("'Model inputs'!A1").unwrap();
    let output = overlay.target("'Model inputs'!B1").unwrap();
    overlay.set_value(input, 7.).unwrap();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(14.)
    );
    let count = overlay.cached_cells();
    let mut manual = book.model().clone();
    manual.apply(overlay.changes()).unwrap();
    let bytes = overlay.save_xlsx().to_buffer().unwrap();
    let exported = Workbook::from_bytes(&bytes).unwrap();
    assert_eq!(
        exported,
        Workbook::from_bytes(
            &Workbook::from_model(manual)
                .save_xlsx()
                .to_buffer()
                .unwrap()
        )
        .unwrap()
    );
    assert_eq!(saved(&exported, "B1"), CellValue::Number(14.));
    assert_eq!(overlay.cached_cells(), count);
    let mut warm = overlay.evaluation_with_profiling(&context, true).unwrap();
    assert_eq!(warm.cell(output).unwrap(), CellValue::Number(14.));
    assert_eq!(warm.finish().execution.unwrap().formula_executions, 0);
    overlay.clear_calculated_cache();
    assert_eq!(overlay.edited_cells(), 1);
    overlay.discard_edits();
    assert_eq!(overlay.edited_cells(), 0);
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(6.)
    );
}

#[test]
fn export_shrinking_spills_cleans_owned_children_and_remains_usable() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("E1".parse().unwrap(), "SEQUENCE(3)")
        .unwrap();
    book.recalculate().unwrap();
    let mut overlay = book.overlay().build().unwrap();
    let anchor = overlay.target("'Model inputs'!E1").unwrap();
    overlay.set_formula(anchor, "SEQUENCE(1)").unwrap();
    let context = EvaluationContext::default();
    overlay.evaluation(&context).unwrap().cell(anchor).unwrap();
    let exported = Workbook::from_bytes(&overlay.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "E1"), CellValue::Number(1.));
    for address in ["E2", "E3"] {
        assert!(
            exported
                .stored_cell("Model inputs", address.parse().unwrap())
                .is_none_or(|c| c.value == CellValue::Blank)
        );
    }
    assert_eq!(saved(&book, "E3"), CellValue::Number(3.));
    overlay.set_formula(anchor, "SEQUENCE(2)").unwrap();
    overlay.evaluation(&context).unwrap().cell(anchor).unwrap();
    let exported = Workbook::from_bytes(&overlay.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "E2"), CellValue::Number(2.));
}

#[test]
fn snapshot_convenience_reports_recovery_and_preserves_saved_policy() {
    let book = book();
    let policy = EvaluationPolicy {
        builtin: poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation,
        ..Default::default()
    };
    let mut overlay = book.overlay().policy(policy.clone()).build().unwrap();
    let input = overlay.target("'Model inputs'!A1").unwrap();
    let output = overlay.target("'Model inputs'!B1").unwrap();
    overlay.set_value(input, 8.).unwrap();
    overlay.compile_all();
    let bytes = overlay.save_snapshot().to_buffer().unwrap();
    assert!(
        Workbook::restore_snapshot()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&bytes)
            .is_ok()
    );
    let restored = Workbook::restore_snapshot().from_bytes(&bytes).unwrap();
    assert_eq!(
        restored.prepared_status(),
        artifact::PreparedStatus::Restored
    );
    let mut restored = restored
        .into_evaluation(EvaluationContext::default())
        .unwrap();
    assert_eq!(restored.evaluation.policy(), &policy);
    assert_eq!(
        restored.evaluation.cell(output).unwrap(),
        CellValue::Number(16.)
    );
    let exported =
        Workbook::from_bytes(&restored.evaluation.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "A1"), CellValue::Number(8.));
    let mut incompatible = bytes.clone();
    incompatible[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(
        Workbook::restore_snapshot()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&incompatible)
            .is_err()
    );
    let mut recovered = Workbook::restore_snapshot()
        .from_bytes(&incompatible)
        .unwrap()
        .into_evaluation(EvaluationContext::default())
        .unwrap();
    assert_eq!(
        recovered.prepared_status,
        artifact::PreparedStatus::DiscardedIncompatible
    );
    assert_eq!(
        recovered.evaluation.cell(output).unwrap(),
        CellValue::Number(16.)
    );
    let source = book.save_snapshot().to_buffer().unwrap();
    assert!(
        Workbook::restore_snapshot()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&source)
            .is_err()
    );
    assert_eq!(
        Workbook::restore_snapshot()
            .from_bytes(&source)
            .unwrap()
            .prepared_status(),
        artifact::PreparedStatus::Absent
    );
    let fallback = Workbook::restore_snapshot()
        .from_bytes(&source)
        .unwrap()
        .into_evaluation_with_policy(EvaluationContext::default(), policy.clone())
        .unwrap();
    assert_eq!(fallback.evaluation.policy(), &policy);
    let source_only = Workbook::restore_snapshot()
        .prepared_policy(artifact::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap();
    assert_eq!(
        source_only.prepared_status(),
        artifact::PreparedStatus::DiscardedByRequest
    );
}

#[test]
fn builder_precedence_profiling_and_new_cycle_are_explicit() {
    let book = book();
    let context = EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        ..Default::default()
    };
    let mut evaluation = book
        .evaluation()
        .limits(poirot::calculation::EvaluationLimits::default().max_work(0))
        .context(context.clone())
        .profiling(true)
        .build()
        .unwrap();
    let input = evaluation.target("'Model inputs'!A1").unwrap();
    let output = evaluation.target("'Model inputs'!B1").unwrap();
    assert_eq!(evaluation.context().as_of_time, std::time::UNIX_EPOCH);
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(6.));
    evaluation.set_value(input, 10.).unwrap();
    evaluation.clear_calculated_cache();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(20.));
    evaluation.discard_edits();
    evaluation.start_cycle(context.clone()).unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(6.));
    assert!(evaluation.finish().execution.unwrap().formula_executions > 0);
    let mut refused = book
        .evaluation()
        .context(context.clone())
        .limits(poirot::calculation::EvaluationLimits::default().max_work(0))
        .build()
        .unwrap();
    assert!(refused.cell(output).is_err());
    // Export is not calculation: it succeeds even while this cycle is failed.
    let bytes = refused.save_xlsx().to_buffer().unwrap();
    assert_eq!(
        saved(&Workbook::from_bytes(&bytes).unwrap(), "B1"),
        CellValue::Number(900.)
    );
    refused.start_cycle(context).unwrap();
    assert_eq!(refused.cell(output).unwrap(), CellValue::Number(6.));
}

#[test]
fn export_validation_failure_does_not_touch_destination_or_scenario() {
    let book = book();
    let mut evaluation = book.evaluation().build().unwrap();
    let output = evaluation.target("'Model inputs'!B1").unwrap();
    evaluation.cell(output).unwrap();
    let path = std::env::temp_dir().join(format!(
        "poirot-scenario-export-{}.xlsx",
        std::process::id()
    ));
    std::fs::write(&path, b"keep destination").unwrap();
    let options = poirot_io::WriteOptions {
        max_output_bytes: 0,
        ..Default::default()
    };
    assert!(
        evaluation
            .save_xlsx()
            .options(options)
            .write_file(&path)
            .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"keep destination");
    std::fs::remove_file(path).unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(6.));
}

#[test]
fn staged_export_changes_fail_atomically_and_owned_edit_application_is_explicit() {
    let book = book();
    let address = "A1".parse().unwrap();
    assert!(
        book.model()
            .with_changes([
                poirot::editing::WorkbookChange::SetValue {
                    sheet: 0,
                    address,
                    value: CellValue::Number(100.)
                },
                poirot::editing::WorkbookChange::SetValue {
                    sheet: 100,
                    address,
                    value: CellValue::Number(200.)
                },
            ])
            .is_err()
    );
    assert_eq!(saved(&book, "A1"), CellValue::Number(3.));
    let mut overlay = poirot::calculation::Overlay::from_overlay(
        poirot::eval::overlay::Overlay::with_workbook(book.clone())
            .build()
            .unwrap(),
    );
    let input = overlay.target("'Model inputs'!A1").unwrap();
    let output = overlay.target("'Model inputs'!B1").unwrap();
    overlay.set_value(input, 11.).unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(22.)
    );
    assert_eq!(overlay.apply_edits_to_workbook().unwrap(), 1);
    assert_eq!(overlay.edited_cells(), 0);
    assert_eq!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .get(output.address)
            .unwrap()
            .value,
        CellValue::Number(900.)
    );
    assert_eq!(
        overlay.cached_value(output).as_deref(),
        Some(&CellValue::Number(22.))
    );
}

#[test]
fn export_captures_state_now_without_recalculation_or_cycle_reset() {
    let book = book();
    let mut evaluation = book.evaluation().profiling(true).build().unwrap();
    let input = evaluation.target("'Model inputs'!A1").unwrap();
    let output = evaluation.target("'Model inputs'!B1").unwrap();
    evaluation.set_value(input, 5.).unwrap();
    evaluation.cell(output).unwrap();
    let pending = evaluation.save_xlsx();
    // Direct reads after capture keep the cycle's existing work and result.
    evaluation.cell(output).unwrap();
    let report = evaluation.finish();
    assert_eq!(report.execution.unwrap().formula_executions, 1);
    assert_eq!(
        saved(
            &Workbook::from_bytes(&pending.to_buffer().unwrap()).unwrap(),
            "B1"
        ),
        CellValue::Number(10.)
    );
    let mut overlay = book.overlay().build().unwrap();
    overlay.set_value(input, 7.).unwrap();
    let pending = overlay.save_xlsx();
    overlay.set_value(input, 9.).unwrap();
    let captured = Workbook::from_bytes(&pending.to_buffer().unwrap()).unwrap();
    let later = Workbook::from_bytes(&overlay.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&captured, "A1"), CellValue::Number(7.));
    assert_eq!(saved(&later, "A1"), CellValue::Number(9.));
}

#[test]
fn wrappers_preserve_owned_and_shared_advanced_modes() {
    let book = std::sync::Arc::new(book().into_model());
    let direct = poirot::eval::overlay::Evaluation::with_workbook(std::sync::Arc::clone(&book))
        .build_calculation(EvaluationContext::default())
        .unwrap();
    let mut direct = poirot::calculation::Evaluation::from_calculation(direct);
    let output = direct.target("'Model inputs'!B1").unwrap();
    assert_eq!(direct.cell(output).unwrap(), CellValue::Number(6.));
    assert!(!direct.save_xlsx().to_buffer().unwrap().is_empty());
    let overlay = poirot::eval::overlay::Overlay::with_workbook(book)
        .build()
        .unwrap();
    let mut overlay = poirot::calculation::Overlay::from_overlay(overlay);
    assert!(!overlay.save_xlsx().to_buffer().unwrap().is_empty());
    assert!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .finish()
            .execution
            .is_none()
    );
}

#[test]
fn discarding_direct_edits_restores_baseline_saved_spill_children_for_export() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("E1".parse().unwrap(), "SEQUENCE(3)")
        .unwrap();
    book.recalculate().unwrap();
    let mut evaluation = book.evaluation().build().unwrap();
    let anchor = evaluation.target("'Model inputs'!E1").unwrap();
    evaluation.set_formula(anchor, "SEQUENCE(1)").unwrap();
    evaluation.cell(anchor).unwrap();
    evaluation.discard_edits();
    let exported = Workbook::from_bytes(&evaluation.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "E3"), CellValue::Number(3.));
    let original_export = Workbook::from_bytes(&book.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(exported, original_export);
}

#[test]
fn canonical_types_cover_direct_and_incremental_scenarios() {
    // Arithmetic expectations: B1 = A1 * 2, C1 = A1 * 3.
    let book = book();
    let builder: poirot::calculation::EvaluationBuilder<'_> = book.evaluation();
    let mut direct: poirot::calculation::Evaluation<'_> = builder.profiling(true).build().unwrap();
    let input = direct.target("'Model inputs'!A1").unwrap();
    let twice = direct.target("'Model inputs'!B1").unwrap();
    let triple = direct.target("'Model inputs'!C1").unwrap();
    direct.set_value(input, 4.).unwrap();
    let values = direct.outputs(&[triple, twice]).unwrap();
    assert_eq!(values[0].value, CellValue::Number(12.));
    assert_eq!(values[1].value, CellValue::Number(8.));
    let exported = Workbook::from_bytes(&direct.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "B1"), CellValue::Number(8.));
    let report: poirot::calculation::EvaluationReport<()> = direct.finish();
    assert!(report.result.is_ok());
    assert!(report.execution.is_some());

    let builder: poirot::calculation::OverlayBuilder<'_> = book.overlay();
    let mut overlay: poirot::calculation::Overlay<'_> = builder.build().unwrap();
    overlay.edit(|edits| edits.set_value(input, 5.)).unwrap();
    let context = EvaluationContext::default();
    let mut cycle: poirot::calculation::EvaluationCycle<'_> =
        overlay.evaluation_with_profiling(&context, true).unwrap();
    let values = cycle.outputs(&[twice, triple]).unwrap();
    assert_eq!(values[0].value, CellValue::Number(10.));
    assert_eq!(values[1].value, CellValue::Number(15.));
    let report: poirot::calculation::EvaluationReport<()> = cycle.finish();
    assert!(report.result.is_ok());
    assert!(report.execution.is_some());
    let exported = Workbook::from_bytes(&overlay.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(saved(&exported, "B1"), CellValue::Number(10.));
    assert_eq!(saved(&book, "A1"), CellValue::Number(3.));
    assert_eq!(saved(&book, "B1"), CellValue::Number(900.));
}
