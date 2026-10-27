//! Public opt-in quota, cycle, cache-admission, and profiling contracts.
use poirot::calculation::{EvaluationContext, EvaluationError, EvaluationLimits};
use poirot::eval::overlay::Evaluation;
use poirot::eval::overlay::Overlay;
use poirot::eval::{BudgetKind, EvaluationBudgets, ResourceFailureKind};
use poirot::{CellValue, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Data").unwrap();
    sheet.set_value("A1", 2.).unwrap();
    for (address, formula) in [
        ("B1", "A1+1"),
        ("C1", "B1+1"),
        ("D1", "\"hello\""),
        ("F1", "SEQUENCE(3)"),
        ("H1", "IFERROR(SUM(SEQUENCE(20)),42)"),
    ] {
        sheet.set_formula(address, formula).unwrap();
    }
    book
}
#[test]
fn optional_configuration_is_reusable_and_legacy_sentinels_translate() {
    let limits = EvaluationLimits::default()
        .max_work(0)
        .max_formula_work(12)
        .max_collection_values(3)
        .max_text_bytes(5);
    assert_eq!(limits.work_limit(), Some(0));
    assert_eq!(limits.without_work_limit().work_limit(), None);
    assert_eq!(limits.formula_work_limit(), Some(12));
    assert_eq!(EvaluationBudgets::from(limits).materialized_values, 3);
    assert_eq!(
        EvaluationLimits::from(EvaluationBudgets::default()),
        EvaluationLimits::default()
    );
    let context = EvaluationContext::default().limits(limits);
    assert_eq!(context.clone().limits, limits);
    assert_eq!(
        EvaluationContext::default()
            .limits(EvaluationLimits::default().max_work(u64::MAX))
            .limits
            .work_limit(),
        Some(u64::MAX)
    );
    assert_eq!(EvaluationLimits::default().work_limit(), None);
}
#[test]
fn loading_defaults_and_independent_named_overrides_have_no_hidden_quotas() {
    let options = poirot_io::ReadOptions::default();
    assert_eq!(options.input_bytes_limit(), None);
    assert_eq!(options.zip_entry_bytes_limit(), None);
    assert_eq!(options.zip_total_bytes_limit(), None);
    assert_eq!(options.zip_entries_limit(), None);
    let xls = options.xls_budget;
    assert_eq!(xls.stream_bytes_limit(), None);
    assert_eq!(xls.metadata_entries_limit(), None);
    assert_eq!(xls.records_limit(), None);
    assert_eq!(xls.strings_limit(), None);
    assert_eq!(xls.string_chars_limit(), None);
    assert_eq!(xls.string_bytes_limit(), None);
    assert_eq!(xls.continuations_limit(), None);
    assert_eq!(xls.cell_text_bytes_limit(), None);
    assert_eq!(xls.cells_limit(), None);
    let options = options
        .max_input_bytes(0)
        .max_zip_entries(5)
        .without_input_bytes_limit();
    assert_eq!(options.input_bytes_limit(), None);
    assert_eq!(options.zip_entries_limit(), Some(5));
    let builder = Workbook::load()
        .max_input_bytes(0)
        .options(options)
        .max_input_bytes(10);
    assert_eq!(builder.read_options().input_bytes_limit(), Some(10));
    assert_eq!(builder.read_options().zip_entries_limit(), Some(5));
    assert_eq!(
        poirot::eval::artifact::SnapshotLimits::default().file_bytes_limit(),
        None
    );
    assert_eq!(
        poirot::eval::artifact::SnapshotLimits::default().decoded_values_limit(),
        None
    );
    assert_eq!(
        poirot::eval::artifact::SnapshotLimits::default().decoded_bytes_limit(),
        None
    );
    assert_eq!(
        poirot_io::WriteOptions::default().max_output_bytes,
        32 * 1024 * 1024
    );
    assert_eq!(
        poirot::eval::artifact::SnapshotLimits::for_writing().max_file_bytes,
        64 * 1024 * 1024
    );
}
#[test]
fn source_byte_quotas_have_exact_zero_and_cleared_boundaries() {
    let bytes = book().save_xlsx().to_buffer().unwrap();
    assert!(
        Workbook::load()
            .max_input_bytes(bytes.len() as u64)
            .from_bytes(&bytes)
            .is_ok()
    );
    for limit in [0, bytes.len() as u64 - 1] {
        let error = Workbook::load()
            .max_input_bytes(limit)
            .from_bytes(&bytes)
            .unwrap_err();
        match error {
            poirot::files::IoError::Quota(failure) => {
                assert_eq!(failure.limit, limit);
                assert_eq!(failure.requested, bytes.len() as u64);
                assert_eq!(failure.resource, poirot_io::IoResource::InputBytes);
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(
        Workbook::load()
            .max_input_bytes(0)
            .without_input_bytes_limit()
            .from_bytes(&bytes)
            .is_ok()
    );
}
#[test]
fn ordinary_profiled_and_bounded_modes_agree_and_reports_are_honest() {
    let book = book();
    for (profiling, limits) in [
        (false, EvaluationLimits::default()),
        (true, EvaluationLimits::default()),
        (false, EvaluationLimits::default().max_work(100_000)),
    ] {
        let mut calculation = Evaluation::with_workbook(&book)
            .build_calculation_with_profiling(
                EvaluationContext::default().limits(limits),
                profiling,
            )
            .unwrap();
        let output = calculation.target("Data!C1").unwrap();
        assert_eq!(calculation.cell(output).unwrap(), CellValue::Number(4.));
        let report = calculation.finish();
        assert!(report.result.is_ok());
        assert_eq!(report.usage.is_some(), profiling);
        assert_eq!(report.execution.is_some(), profiling);
        if let Some(usage) = report.usage {
            assert!(usage.work_units > 0);
        }
    }
    let mut legacy = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let target = legacy.target("Data!C1").unwrap();
    legacy.cell(target).unwrap();
    assert!(legacy.finish().outcome.usage().work_units > 0);
}
#[test]
fn work_is_shared_and_refusal_requires_explicit_begin_preserving_edits() {
    let book = book();
    let mut measured = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let b = measured.target("Data!B1").unwrap();
    let c = measured.target("Data!C1").unwrap();
    measured.cell(b).unwrap();
    let work = measured.finish().outcome.usage().work_units;
    let context = EvaluationContext::default().limits(EvaluationLimits::default().max_work(work));
    let mut calculation = Evaluation::with_workbook(&book)
        .build_calculation(context)
        .unwrap();
    assert_eq!(calculation.cell(b).unwrap(), CellValue::Number(3.));
    let failure = calculation.cell(c).unwrap_err();
    assert_eq!(failure.budget().unwrap().kind, BudgetKind::RunWork);
    calculation.clear_cache();
    let input = calculation.target("Data!A1").unwrap();
    calculation.set_value(input, 9.).unwrap();
    assert_eq!(
        (calculation.cell(b).unwrap_err()).budget().unwrap(),
        failure.budget().unwrap()
    );
    calculation.begin(EvaluationContext::default()).unwrap();
    assert_eq!(calculation.cell(b).unwrap(), CellValue::Number(10.));
    let mut zero = Evaluation::with_workbook(&book)
        .build_calculation(
            EvaluationContext::default().limits(EvaluationLimits::default().max_work(0)),
        )
        .unwrap();
    assert_eq!(zero.cell(b).unwrap_err().budget().unwrap().consumed, 0);
}
#[test]
fn cached_text_and_spill_footprints_obey_tighter_result_admission() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let text = overlay.target("Data!D1").unwrap();
    let child = overlay.target("Data!F3").unwrap();
    let context = EvaluationContext::default();
    {
        let mut cycle = overlay.calculation(&context).unwrap();
        assert_eq!(cycle.cell(text).unwrap(), CellValue::Text("hello".into()));
        assert_eq!(cycle.cell(child).unwrap(), CellValue::Number(3.));
    }
    let tighter = context
        .clone()
        .limits(EvaluationLimits::default().max_text_bytes(4));
    assert_eq!(
        overlay
            .calculation(&tighter)
            .unwrap()
            .cell(text)
            .unwrap_err()
            .budget()
            .unwrap()
            .kind,
        BudgetKind::TextBytes
    );
    let tighter = context
        .clone()
        .limits(EvaluationLimits::default().max_collection_values(2));
    assert_eq!(
        overlay
            .calculation(&tighter)
            .unwrap()
            .cell(child)
            .unwrap_err()
            .budget()
            .unwrap()
            .kind,
        BudgetKind::Materialization
    );
    assert_eq!(
        overlay.calculation(&context).unwrap().cell(child).unwrap(),
        CellValue::Number(3.)
    );
}
#[test]
fn tighter_retention_refuses_before_altering_valid_overlay_state() {
    let book = book();
    let mut overlay = Overlay::with_workbook(&book).build().unwrap();
    let output = overlay.target("Data!C1").unwrap();
    let context = EvaluationContext::default();
    overlay.calculation(&context).unwrap().cell(output).unwrap();
    let retained = overlay.cached_cells();
    assert!(retained > 0);
    let mut tighter = context.clone();
    tighter.retention = tighter.retention.max_cached_cells(0);
    assert!(matches!(
        overlay.calculation(&tighter),
        Err(poirot::calculation::OverlayError::Evaluation(
            EvaluationError::MemoRetentionLimit { .. }
        ))
    ));
    assert_eq!(overlay.cached_cells(), retained);
    let mut run = overlay.calculation_with_profiling(&context, true).unwrap();
    assert_eq!(run.cell(output).unwrap(), CellValue::Number(4.));
    assert!(run.finish().execution.unwrap().external_cache_hits > 0);
}
#[test]
fn iferror_cannot_swallow_caller_refusals_or_native_regex_guards() {
    let book = book();
    let context =
        EvaluationContext::default().limits(EvaluationLimits::default().max_collection_values(5));
    let mut calculation = Evaluation::with_workbook(&book)
        .build_calculation(context)
        .unwrap();
    let target = calculation.target("Data!H1").unwrap();
    assert_eq!(
        calculation.cell(target).unwrap_err().budget().unwrap().kind,
        BudgetKind::Materialization
    );
    let context = EvaluationContext::default()
        .regex_settings(poirot::calculation::RegexSettings::default().max_match_steps(0));
    let mut calculation = Evaluation::with_workbook(&book)
        .build_calculation(context)
        .unwrap();
    calculation
        .set_formula(target, "IFERROR(REGEXTEST(\"aaa\",\"a+\"),42)")
        .unwrap();
    assert!(matches!(
        calculation.cell(target),
        Err(EvaluationError::Resource {
            kind: ResourceFailureKind::BackendGuard,
            ..
        })
    ));
}
#[test]
fn snapshot_refusals_are_not_malformed_payload_errors() {
    let bytes = book().save_snapshot().to_buffer().unwrap();
    let limits = poirot::eval::artifact::SnapshotLimits::default().max_decoded_values(0);
    let error = poirot::eval::artifact::SnapshotLoadBuilder::default()
        .limits(limits)
        .from_bytes(&bytes)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        poirot::eval::artifact::SnapshotError::Quota(_)
    ));
    let limits = limits.without_decoded_values_limit();
    assert!(
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .limits(limits)
            .from_bytes(&bytes)
            .is_ok()
    );
}

#[test]
fn native_random_effects_agree_without_retaining_profiling_traces() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("J1", "RAND()+RAND()")
        .unwrap();
    let context = EvaluationContext {
        randomness_seed: 77,
        ..Default::default()
    };
    let mut results = Vec::new();
    for profiling in [false, true] {
        let mut calculation = Evaluation::with_workbook(&book)
            .build_calculation_with_profiling(context.clone(), profiling)
            .unwrap();
        let output = calculation.target("Data!J1").unwrap();
        results.push(calculation.cell(output).unwrap());
        let report = calculation.finish();
        if profiling {
            assert_eq!(report.usage.unwrap().random_accesses[0].draws, 2);
        } else {
            assert!(report.usage.is_none());
        }
    }
    assert_eq!(results[0], results[1]);
}
#[test]
fn configured_zip_quotas_have_exact_admission_and_independent_diagnostics() {
    let bytes = book().save_xlsx().to_buffer().unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let count = zip.len();
    let mut largest = 0;
    let mut total = 0;
    for index in 0..count {
        let entry = zip.by_index(index).unwrap();
        largest = largest.max(entry.size());
        total += entry.size();
    }
    let exact = poirot_io::ReadOptions::default()
        .max_zip_entries(count)
        .max_zip_entry_bytes(largest)
        .max_zip_total_bytes(total);
    assert!(Workbook::load().options(exact).from_bytes(&bytes).is_ok());
    for (options, resource) in [
        (exact.max_zip_entries(0), poirot_io::IoResource::ZipEntries),
        (
            exact.max_zip_entry_bytes(largest - 1),
            poirot_io::IoResource::ZipEntryBytes,
        ),
        (
            exact.max_zip_total_bytes(total - 1),
            poirot_io::IoResource::ZipTotalBytes,
        ),
    ] {
        let error = Workbook::load()
            .options(options)
            .from_bytes(&bytes)
            .unwrap_err();
        assert!(
            matches!(error, poirot::files::IoError::Quota(failure) if failure.resource == resource && failure.requested > failure.limit.saturating_sub(failure.consumed))
        );
    }
}
#[test]
fn invalid_nesting_configuration_fails_before_work_and_low_level_refusals_remain_failed() {
    let book = book();
    let invalid = EvaluationContext::default().limits(EvaluationLimits::default().max_depth(129));
    assert!(
        matches!(Evaluation::with_workbook(&book).build_calculation(invalid), Err(poirot::calculation::OverlayError::Evaluation(EvaluationError::Budget(failure))) if failure.kind == BudgetKind::Configuration)
    );
    let policy = poirot::eval::PreparationPolicy::default();
    let plan = poirot::eval::FormulaCompiler::new().compile_workbook(&book, &policy);
    let view = poirot::eval::WorkbookView::new(&book, &plan, &policy).unwrap();
    let mut run = poirot::eval::EvaluationRun::new(
        view,
        Default::default(),
        EvaluationBudgets {
            text_bytes: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let text = poirot::calculation::EvaluationTarget {
        sheet: 0,
        address: "D1".parse().unwrap(),
    };
    let literal = poirot::calculation::EvaluationTarget {
        sheet: 0,
        address: "A1".parse().unwrap(),
    };
    let failure = run.evaluate_cell(text).unwrap_err();
    assert_eq!(
        (run.evaluate_cell(literal).unwrap_err()).budget().unwrap(),
        failure.budget().unwrap()
    );
    assert!(matches!(
        run.finish(Ok(())).outcome,
        poirot::eval::EvaluationOutcome::Aborted { .. }
    ));
}
