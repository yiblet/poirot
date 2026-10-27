//! Public workflows use only names deliberately exposed by the root facade.
use poirot::calculation::{Evaluation, EvaluationContext, Overlay};
use poirot::{CellError, CellValue, Workbook};
use poirot::{calculation, files, snapshots};
use std::error::Error;
use std::sync::Arc;

/// A small model with documented arithmetic and worksheet-error expectations.
fn fixture_book() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs").unwrap();
    sheet.set_value("A1", 21.).unwrap();
    sheet.set_formula("B1", "A1*2").unwrap();
    sheet.set_formula("C1", "1/0").unwrap();
    book
}

#[test]
fn recalculated_scalars_are_available_to_customer_business_code()
-> Result<(), Box<dyn std::error::Error>> {
    let mut book = fixture_book();
    book.sheet_by_name_mut("Inputs")
        .ok_or("missing Inputs")?
        .set_formula("D1", "B1>40")?;
    book.recalculate()?;
    let amount = book
        .stored_cell("Inputs", "B1")?
        .ok_or("missing amount")?
        .value
        .as_number()
        .ok_or("expected numeric amount")?;
    let active = book
        .stored_cell("Inputs", "D1")?
        .ok_or("missing active")?
        .value
        .as_boolean()
        .ok_or("expected boolean active")?;
    assert_eq!(amount, 42.);
    assert!(active);
    Ok(())
}

#[test]
fn file_builders_and_supporting_types_are_available_by_workflow() {
    let book = fixture_book();
    let builder: files::XlsxSaveBuilder<'_> = book.save_xlsx();
    let bytes = builder.to_buffer().unwrap();
    let loader: files::LoadBuilder = Workbook::load();
    let mut loaded = loader.from_bytes(&bytes).unwrap();
    loaded.recalculate().unwrap();
    assert_eq!(
        loaded.stored_cell("Inputs", "B1").unwrap().unwrap().value,
        CellValue::Number(42.)
    );
    assert!(matches!(
        Workbook::from_bytes(b"invalid"),
        Err(files::IoError::Unsupported(_))
    ));
}

#[test]
fn owned_shared_and_borrowed_sources_support_reports_and_failure_recovery() {
    let book = fixture_book();
    let builder: calculation::EvaluationBuilder<'_> = book.evaluation();
    let mut direct = builder
        .limits(calculation::EvaluationLimits::default().max_work(0))
        .profiling(true)
        .build()
        .unwrap();
    let output: calculation::EvaluationTarget = direct.target("Inputs!B1").unwrap();
    let failure: calculation::EvaluationError = direct.cell(output).unwrap_err();
    assert!(matches!(failure, calculation::EvaluationError::Budget(_)));
    direct.start_cycle(EvaluationContext::default()).unwrap();
    assert_eq!(direct.cell(output).unwrap(), CellValue::Number(42.));
    let error = direct.target("Inputs!C1").unwrap();
    assert_eq!(
        direct.cell(error).unwrap(),
        CellValue::Error(CellError::Div0)
    );
    let report: calculation::EvaluationReport<()> = direct.finish();
    assert!(report.result.is_ok());
    assert!(report.usage.is_some());
    assert!(report.execution.is_some());

    let source = Arc::new(book);
    let mut shared = Evaluation::with_workbook(Arc::clone(&source))
        .build()
        .unwrap();
    assert_eq!(shared.cell(output).unwrap(), CellValue::Number(42.));
    let mut overlay = Overlay::with_workbook(source).build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    overlay
        .edit(|edits: &mut calculation::OverlayEdits<'_, '_, _>| edits.set_value(input, 24.))
        .unwrap();
    let context = EvaluationContext::default();
    let mut cycle: calculation::EvaluationCycle<'_> =
        overlay.evaluation_with_profiling(&context, true).unwrap();
    assert_eq!(cycle.cell(output).unwrap(), CellValue::Number(48.));
    assert!(cycle.finish().execution.is_some());
    let mut owned = Overlay::with_workbook(fixture_book()).build().unwrap();
    owned.set_value(input, 30.).unwrap();
    assert_eq!(owned.apply_edits_to_workbook().unwrap(), 1);
}

#[test]
fn standalone_formula_text_keeps_anchor_source_and_blank_expression_semantics() {
    let book = fixture_book();
    let address = "A1".parse().unwrap();
    let result = calculation::run_formula(
        &book,
        0,
        address,
        "A1*2",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        result,
        calculation::EvaluationOutcome::Complete {
            value: CellValue::Number(42.),
            ..
        }
    ));
    let blank = calculation::run_formula(
        &book,
        0,
        address,
        "D1",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        blank,
        calculation::EvaluationOutcome::Complete {
            value: CellValue::Blank,
            ..
        }
    ));
    assert_eq!(
        book.stored_cell("Inputs", address).unwrap().unwrap().value,
        CellValue::Number(21.)
    );
}

#[test]
fn snapshots_recover_authored_source_and_reuse_incremental_state() {
    let book = fixture_book();
    let mut overlay = book.overlay().build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    let output = overlay.target("Inputs!B1").unwrap();
    overlay.set_value(input, 24.).unwrap();
    overlay
        .evaluation(&EvaluationContext::default())
        .unwrap()
        .cell(output)
        .unwrap();
    let bytes = overlay.save_snapshot().to_buffer().unwrap();
    let restored: snapshots::RestoredWorkbook = Workbook::restore_snapshot()
        .prepared_policy(snapshots::PreparedPolicy::RequireCompatible)
        .from_bytes(&bytes)
        .unwrap();
    assert_eq!(
        restored.prepared_status(),
        snapshots::PreparedStatus::Restored
    );
    let mut warm = restored.into_overlay().unwrap();
    assert_eq!(
        warm.cached_value(output).as_deref(),
        Some(&CellValue::Number(48.))
    );
    let context = EvaluationContext::default();
    let mut cycle = warm.evaluation_with_profiling(&context, true).unwrap();
    assert_eq!(cycle.cell(output).unwrap(), CellValue::Number(48.));
    assert_eq!(cycle.finish().execution.unwrap().formula_executions, 0);
    let source = Workbook::restore_snapshot()
        .prepared_policy(snapshots::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap()
        .into_workbook()
        .unwrap();
    assert_eq!(
        source
            .stored_cell("Inputs", input.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(24.)
    );
    assert_eq!(
        source
            .stored_cell("Inputs", output.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Blank
    );
    let source_bytes = source.save_snapshot().to_buffer().unwrap();
    assert!(
        Workbook::restore_snapshot()
            .prepared_policy(snapshots::PreparedPolicy::RequireCompatible)
            .from_bytes(&source_bytes)
            .is_err()
    );
    assert!(Workbook::restore_snapshot().from_bytes(b"invalid").is_err());
}

/// Build an application provider whose lazy first argument preserves reference semantics.
fn registry(signature: &str) -> calculation::extensions::ExtensionRegistry {
    use calculation::extensions::*;
    let mut registry = ExtensionRegistry::new();
    registry
        .register(
            ExtensionNamespace::UserDefined,
            "USER_FIRST",
            signature,
            ExtensionContract {
                arity: Arity {
                    minimum: 2,
                    maximum: 2,
                },
                evaluation: ExtensionEvaluationStrategy::Deferred,
                capabilities: ExtensionCapabilities {
                    references: true,
                    arrays: false,
                    context: false,
                },
                cacheability: ExtensionCacheability::Deterministic,
            },
            |_: &mut dyn ExtensionHost| Ok(ExtensionOutput::Argument(0)),
        )
        .unwrap();
    registry
}

#[test]
fn extension_providers_can_be_shared_and_reattached_to_snapshots() {
    let registry = registry("first/v1");
    let registry = Arc::new(registry);
    let mut book = fixture_book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "USER_FIRST(A1,1/0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book)
        .shared_extensions(Arc::clone(&registry))
        .build()
        .unwrap();
    let output = overlay.target("Inputs!B1").unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        CellValue::Number(21.)
    );
    let bytes = overlay.save_snapshot().to_buffer().unwrap();
    assert!(
        Workbook::restore_snapshot()
            .from_bytes(&bytes)
            .unwrap()
            .into_overlay()
            .is_err()
    );
    let mut restored = Workbook::restore_snapshot()
        .shared_extensions(Arc::clone(&registry))
        .from_bytes(&bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    assert_eq!(
        restored
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        CellValue::Number(21.)
    );
    let context = EvaluationContext::default();
    let mut cycle = restored.evaluation_with_profiling(&context, true).unwrap();
    assert_eq!(cycle.cell(output).unwrap(), CellValue::Number(21.));
    assert_eq!(cycle.finish().execution.unwrap().formula_executions, 0);
    let mut direct = Workbook::restore_snapshot()
        .shared_extensions(Arc::clone(&registry))
        .from_bytes(&bytes)
        .unwrap()
        .into_evaluation(EvaluationContext::default())
        .unwrap()
        .evaluation;
    assert_eq!(direct.cell(output).unwrap(), CellValue::Number(21.));
}

#[test]
fn registry_attachment_covers_borrowed_and_shared_builders_and_source_recovery() {
    let registry = Arc::new(registry("first/v1"));
    let mut book = fixture_book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "USER_FIRST(A1,1/0)")
        .unwrap();
    let mut borrowed = book.evaluation().extensions(&registry).build().unwrap();
    let output = borrowed.target("Inputs!B1").unwrap();
    assert_eq!(borrowed.cell(output).unwrap(), CellValue::Number(21.));
    let mut borrowed_overlay = book.overlay().extensions(&registry).build().unwrap();
    assert_eq!(
        borrowed_overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        CellValue::Number(21.)
    );
    let mut shared = Evaluation::with_workbook(Arc::new(book.clone()))
        .shared_extensions(Arc::clone(&registry))
        .build()
        .unwrap();
    assert_eq!(shared.cell(output).unwrap(), CellValue::Number(21.));
    let mut owned = book
        .clone()
        .into_evaluation()
        .extensions(&registry)
        .build()
        .unwrap();
    assert_eq!(owned.cell(output).unwrap(), CellValue::Number(21.));
    let bytes = shared.save_snapshot().to_buffer().unwrap();
    for into_overlay in [false, true] {
        let restored = Workbook::restore_snapshot()
            .prepared_policy(snapshots::PreparedPolicy::SourceOnly)
            .shared_extensions(Arc::clone(&registry))
            .from_bytes(&bytes)
            .unwrap();
        assert_eq!(
            restored.prepared_status(),
            snapshots::PreparedStatus::DiscardedByRequest
        );
        if into_overlay {
            let mut overlay = restored.into_overlay().unwrap();
            assert_eq!(
                overlay
                    .evaluation(&EvaluationContext::default())
                    .unwrap()
                    .cell(output)
                    .unwrap(),
                CellValue::Number(21.)
            );
        } else {
            let mut direct = restored
                .into_evaluation(EvaluationContext::default())
                .unwrap()
                .evaluation;
            assert_eq!(direct.cell(output).unwrap(), CellValue::Number(21.));
        }
    }
    let incompatible = Arc::new(registry_with_different_signature());
    for into_overlay in [false, true] {
        let restored = Workbook::restore_snapshot()
            .shared_extensions(Arc::clone(&incompatible))
            .from_bytes(&bytes)
            .unwrap();
        if into_overlay {
            assert!(matches!(
                restored.into_overlay(),
                Err(snapshots::SnapshotError::Workspace(
                    calculation::OverlayError::RegistryMismatch
                ))
            ));
        } else {
            assert!(matches!(
                restored.into_evaluation(EvaluationContext::default()),
                Err(snapshots::SnapshotError::Workspace(
                    calculation::OverlayError::RegistryMismatch
                ))
            ));
        }
    }
    assert!(
        Workbook::restore_snapshot()
            .from_bytes(&bytes)
            .unwrap()
            .into_evaluation(EvaluationContext::default())
            .is_err()
    );
}

/// Give the same spelling a different implementation revision to test exact binding checks.
fn registry_with_different_signature() -> calculation::extensions::ExtensionRegistry {
    registry("first/v2")
}

#[test]
fn restored_prepared_source_keeps_edits_without_promoting_results() {
    let book = fixture_book();
    let mut overlay = book.overlay().build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    let output = overlay.target("Inputs!B1").unwrap();
    overlay.set_value(input, 24.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        CellValue::Number(48.)
    );
    let bytes = overlay.save_snapshot().to_buffer().unwrap();
    let source = Workbook::restore_snapshot()
        .from_bytes(&bytes)
        .unwrap()
        .into_workbook()
        .unwrap();
    assert_eq!(
        source
            .stored_cell("Inputs", input.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(24.)
    );
    assert_eq!(
        source
            .stored_cell("Inputs", output.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Blank
    );
    let source_bytes = source.save_snapshot().to_buffer().unwrap();
    let mut overlay = Workbook::restore_snapshot()
        .from_bytes(&source_bytes)
        .unwrap()
        .into_overlay()
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        CellValue::Number(48.)
    );
}

#[test]
fn shared_failure_keeps_the_original_source_and_owned_failure_can_recover() {
    let source = Arc::new(fixture_book());
    let mut run = Evaluation::with_workbook(Arc::clone(&source))
        .limits(calculation::EvaluationLimits::default().max_work(0))
        .build()
        .unwrap();
    let input = run.target("Inputs!A1").unwrap();
    let output = run.target("Inputs!B1").unwrap();
    run.set_value(input, 30.).unwrap();
    assert!(run.cell(output).is_err());
    assert!(std::ptr::eq(run.workbook(), source.as_ref()));
    drop(run);
    assert_eq!(Arc::strong_count(&source), 1);
    assert_eq!(
        source
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(21.)
    );
    let mut run = fixture_book()
        .into_evaluation()
        .limits(calculation::EvaluationLimits::default().max_work(0))
        .build()
        .unwrap();
    run.set_value(input, 30.).unwrap();
    assert!(run.cell(output).is_err());
    let baseline = run.into_workbook();
    assert_eq!(
        baseline
            .stored_cell("Inputs", input.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(21.)
    );
}

#[test]
fn direct_style_edits_invalidate_results_and_export_without_touching_baseline() {
    use poirot::styles::CellStyle;
    let book = fixture_book();
    let mut run = book.evaluation().profiling(true).build().unwrap();
    let input = run.target("Inputs!A1").unwrap();
    let output = run.target("Inputs!B1").unwrap();
    run.set_value(input, 24.).unwrap();
    assert_eq!(run.cell(output).unwrap(), CellValue::Number(48.));
    assert!(run.cached_cells() > 0);
    let style = CellStyle {
        number_format: Some("0.00".into()),
        ..Default::default()
    };
    run.set_style(output, style.clone()).unwrap();
    assert_eq!(run.cached_cells(), 0);
    let values = run.outputs(&[output]).unwrap();
    assert_eq!(values[0].value, CellValue::Number(48.));
    assert_eq!(values[0].formatting.as_ref(), Some(&style));
    let before = run.cached_cells();
    let invalid = calculation::EvaluationTarget {
        sheet: usize::MAX,
        address: input.address,
    };
    assert!(run.set_style(invalid, style.clone()).is_err());
    assert_eq!(run.cached_cells(), before);
    let exported = Workbook::from_bytes(&run.save_xlsx().to_buffer().unwrap()).unwrap();
    assert_eq!(
        exported
            .stored_cell("Inputs", output.address)
            .unwrap()
            .unwrap()
            .style,
        style
    );
    assert_eq!(
        exported
            .stored_cell("Inputs", output.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(48.)
    );
    assert_eq!(
        book.stored_cell("Inputs", input.address)
            .unwrap()
            .unwrap()
            .value,
        CellValue::Number(21.)
    );
    assert_eq!(
        book.stored_cell("Inputs", output.address)
            .unwrap()
            .unwrap()
            .style,
        CellStyle::default()
    );
    let report = run.finish();
    assert_eq!(report.execution.unwrap().formula_executions, 2);
    assert!(report.usage.unwrap().work_units > 0);
}

#[test]
fn style_edits_preserve_work_failure_and_authored_inputs_until_a_new_cycle() {
    use poirot::styles::CellStyle;
    let book = fixture_book();
    let mut run = book
        .evaluation()
        .limits(calculation::EvaluationLimits::default().max_work(1))
        .profiling(true)
        .build()
        .unwrap();
    let input = run.target("Inputs!A1").unwrap();
    let output = run.target("Inputs!B1").unwrap();
    run.set_value(input, 24.).unwrap();
    let failure = run.cell(output).unwrap_err();
    let mut control = book
        .evaluation()
        .limits(calculation::EvaluationLimits::default().max_work(1))
        .profiling(true)
        .build()
        .unwrap();
    control.set_value(input, 24.).unwrap();
    let calculation::EvaluationError::Budget(expected) = &failure else {
        panic!("expected metered budget rejection")
    };
    assert_eq!(control.cell(output).unwrap_err().budget(), Some(expected));
    let before = control.finish();
    run.set_style(
        output,
        CellStyle {
            number_format: Some("0.00".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(run.cell(output).unwrap_err().budget(), Some(expected));
    let bytes = run.save_snapshot().to_buffer().unwrap();
    let after = run.finish();
    assert_eq!(after.usage, before.usage);
    assert_eq!(after.execution, before.execution);
    assert_eq!(after.result.unwrap_err().budget(), Some(expected));
    let mut recovered = Workbook::restore_snapshot()
        .from_bytes(&bytes)
        .unwrap()
        .into_evaluation(EvaluationContext::default())
        .unwrap()
        .evaluation;
    assert_eq!(recovered.cell(output).unwrap(), CellValue::Number(48.));
    assert_eq!(
        recovered.outputs(&[output]).unwrap()[0]
            .formatting
            .as_ref()
            .unwrap()
            .number_format
            .as_deref(),
        Some("0.00")
    );
}

/// Accept every workbook export through the same public file contract.
fn export_bytes(save: files::XlsxSaveBuilder<'_>) -> Result<Vec<u8>, files::XlsxSaveError> {
    save.options(files::WriteOptions::default()).to_buffer()
}

#[test]
fn common_save_contract_preserves_owned_capture_after_sources_drop() {
    let direct_save = {
        let book = fixture_book();
        assert!(!export_bytes(book.save_xlsx()).unwrap().is_empty());
        let mut run = book.evaluation().build().unwrap();
        let input = run.target("Inputs!A1").unwrap();
        let output = run.target("Inputs!B1").unwrap();
        run.set_value(input, 7.).unwrap();
        assert_eq!(run.cell(output).unwrap(), CellValue::Number(14.));
        let captured: files::XlsxSaveBuilder<'static> = run.save_xlsx();
        run.set_value(input, 9.).unwrap();
        captured
    };
    let overlay_save = {
        let book = fixture_book();
        let mut run = book.overlay().build().unwrap();
        let input = run.target("Inputs!A1").unwrap();
        run.set_value(input, 11.).unwrap();
        run.save_xlsx()
    };
    for (save, input, output) in [
        (direct_save, 7., CellValue::Number(14.)),
        (overlay_save, 11., CellValue::Blank),
    ] {
        let loaded = Workbook::from_bytes(&export_bytes(save).unwrap()).unwrap();
        assert_eq!(
            loaded.stored_cell("Inputs", "A1").unwrap().unwrap().value,
            CellValue::Number(input)
        );
        assert_eq!(
            loaded.stored_cell("Inputs", "B1").unwrap().unwrap().value,
            output
        );
    }
}

#[test]
fn root_file_policy_and_diagnostics_are_usable_without_component_names() {
    let policy = files::OpcImportPolicy {
        external_relationships: files::ExternalRelationshipPolicy::Reject,
    };
    let bytes = fixture_book().save_xlsx().to_buffer().unwrap();
    let error = Workbook::load()
        .xlsx_policy(policy)
        .max_input_bytes(0)
        .from_bytes(&bytes)
        .unwrap_err();
    let files::IoError::Quota(failure) = error else {
        panic!("expected input quota")
    };
    let failure: files::IoQuotaError = failure;
    assert_eq!(
        (failure.limit, failure.consumed, failure.requested),
        (0, 0, bytes.len() as u64)
    );
    assert_eq!(failure.resource, files::IoResource::InputBytes);
    // Both payload families can be classified through the root domain.
    let encrypted = files::IoError::BiffPreflight(files::PreflightError::EncryptedXlsx);
    assert!(
        std::error::Error::source(&encrypted)
            .unwrap()
            .downcast_ref::<files::PreflightError>()
            .is_some()
    );
    let opc = files::IoError::Opc(files::OpcError::ExternalRelationship {
        owner: None,
        id: "external".into(),
    });
    assert!(
        std::error::Error::source(&opc)
            .unwrap()
            .downcast_ref::<files::OpcError>()
            .is_some()
    );
    let book = fixture_book();
    let destination = std::env::temp_dir()
        .join(format!("poirot-missing-parent-{}", std::process::id()))
        .join("book.xlsx");
    let error = book.save_xlsx().write_file(destination).unwrap_err();
    assert!(
        matches!(&error, files::XlsxSaveError::Io(files::IoError::File(e)) if e.kind() == std::io::ErrorKind::NotFound)
    );
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .downcast_ref::<files::IoError>()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
}
