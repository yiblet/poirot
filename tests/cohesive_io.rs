//! Common workbook persistence paths share builders and preserve source provenance.
use poirot::calculation::EvaluationContext;
use poirot::eval::EvaluationPolicy;
use poirot::eval::artifact;
use poirot::eval::overlay::Evaluation;
use poirot::eval::overlay::Overlay;
use poirot::files::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
use poirot::{CellValue, Workbook};
use poirot_io as io;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Temporary output directory isolated from concurrent persistence tests.
struct TestDirectory(PathBuf);
impl TestDirectory {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "poirot-cohesive-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs").unwrap();
    sheet.set_value("A1".parse().unwrap(), 21.).unwrap();
    sheet
        .set_formula_with_cached_value("B1".parse().unwrap(), "A1*2", CellValue::Number(-1.))
        .unwrap();
    book
}
fn metadata() -> FileMetadata {
    FileMetadata {
        format: FileFormat::Xlsx,
        import_warnings: vec![ImportDiagnostic {
            kind: ImportDiagnosticKind::ExternalLink,
            detail: "external link omitted".into(),
        }],
    }
}
fn context() -> EvaluationContext {
    EvaluationContext {
        as_of_time: std::time::UNIX_EPOCH,
        randomness_seed: 42,
        ..Default::default()
    }
}

#[test]
fn new_and_cloned_workbooks_keep_the_public_type_and_optional_metadata() {
    let mut book = workbook();
    assert!(book.model().file_metadata.is_none());
    book.model_mut().file_metadata = Some(metadata());
    let cloned: Workbook = book.clone();
    assert_eq!(cloned, book);
    assert_eq!(cloned.model().file_metadata, Some(metadata()));
    assert!(!cloned.save_xlsx().to_buffer().unwrap().is_empty());
}

#[test]
fn spreadsheet_file_and_buffer_paths_share_format_detection_and_options() {
    let dir = TestDirectory::new();
    let book = workbook();
    for (name, bytes, format) in [
        (
            "book.xlsx",
            book.save_xlsx().to_buffer().unwrap(),
            FileFormat::Xlsx,
        ),
        (
            "book.xls",
            include_bytes!("../crates/poirot-io/tests/fixtures/poi-StringFormulas.xls").to_vec(),
            FileFormat::Xls,
        ),
    ] {
        let path = dir.0.join(name);
        match format {
            FileFormat::Xlsx => book.save_xlsx().write_file(&path).unwrap(),
            FileFormat::Xls => std::fs::write(&path, &bytes).unwrap(),
        }
        let from_file = Workbook::open(&path).unwrap();
        let from_bytes = Workbook::load()
            .xlsx_policy(io::opc::OpcImportPolicy::default())
            .xls_budget(io::ResourceBudget::default())
            .from_bytes(&bytes)
            .unwrap();
        assert_eq!(from_file, from_bytes);
        let metadata = from_file.model().file_metadata.as_ref().unwrap();
        assert_eq!(metadata.format, format);
        assert!(
            metadata
                .import_warnings
                .iter()
                .any(|d| d.kind == ImportDiagnosticKind::OmittedFeatures)
        );
        if format == FileFormat::Xlsx {
            assert_eq!(
                from_file
                    .sheet(0)
                    .unwrap()
                    .get("B1".parse().unwrap())
                    .unwrap()
                    .value,
                CellValue::Number(-1.)
            );
        }
        let options = io::ReadOptions {
            max_input_bytes: 0,
            ..Default::default()
        };
        assert!(
            Workbook::load()
                .options(options)
                .from_bytes(&bytes)
                .is_err()
        );
        assert!(Workbook::load().options(options).open(path).is_err());
    }
}

#[test]
fn source_snapshots_preserve_typed_metadata_and_never_recalculate() {
    let dir = TestDirectory::new();
    let mut book = workbook();
    book.model_mut().file_metadata = Some(metadata());
    let bytes = book.save_snapshot().to_buffer().unwrap();
    let path = dir.0.join("source.poirot");
    book.save_snapshot().write_file(&path).unwrap();
    assert_eq!(
        Workbook::from_model(
            Workbook::restore_snapshot()
                .from_bytes(&bytes)
                .unwrap()
                .into_artifact()
                .into_parts()
                .0
        ),
        book
    );
    assert_eq!(
        Workbook::from_model(
            Workbook::restore_snapshot()
                .open(&path)
                .unwrap()
                .into_artifact()
                .into_parts()
                .0
        ),
        book
    );
    let restored = poirot::eval::artifact::SnapshotLoadBuilder::default()
        .from_bytes(&bytes)
        .unwrap();
    assert_eq!(restored.prepared_status, artifact::PreparedStatus::Absent);
    assert_eq!(
        restored.workbook().file_metadata,
        book.model().file_metadata
    );
    assert!(restored.into_parts().1.is_none());
    let limits = artifact::SnapshotLimits {
        max_file_bytes: 1,
        ..Default::default()
    };
    assert!(
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .limits(limits)
            .open(&path)
            .is_err()
    );
    std::fs::write(&path, b"preserve destination").unwrap();
    assert!(
        book.save_snapshot()
            .limits(limits)
            .write_file(&path)
            .is_err()
    );
    assert_eq!(std::fs::read(path).unwrap(), b"preserve destination");
}

#[test]
fn overlay_checkpoints_restore_warm_state_without_detached_attachment() {
    let dir = TestDirectory::new();
    let mut book = workbook();
    book.model_mut().file_metadata = Some(metadata());
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    let output = overlay.target("Inputs!B1").unwrap();
    overlay.set_value(input, 30.).unwrap();
    let context = context();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        CellValue::Number(60.)
    );
    let bytes = poirot::eval::artifact::SnapshotSaveBuilder::checkpoint(overlay.snapshot())
        .to_buffer()
        .unwrap();
    let path = dir.0.join("checkpoint.poirot");
    poirot::eval::artifact::SnapshotSaveBuilder::checkpoint(overlay.snapshot())
        .write_file(&path)
        .unwrap();
    for restored in [
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .from_bytes(&bytes)
            .unwrap(),
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .open(path)
            .unwrap(),
    ] {
        assert_eq!(restored.prepared_status, artifact::PreparedStatus::Restored);
        assert_eq!(restored.workbook().file_metadata, Some(metadata()));
        let artifact::RestoredCheckpoint::Prepared(mut restored) =
            restored.into_checkpoint().unwrap()
        else {
            panic!("prepared checkpoint was discarded")
        };
        let mut evaluation = restored.evaluation(&context).unwrap();
        assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(60.));
        assert_eq!(evaluation.usage().formula_executions, 0);
    }
}

#[test]
fn incompatible_checkpoints_recover_edits_with_an_explicit_typed_outcome() {
    let mut book = workbook();
    book.model_mut().file_metadata = Some(metadata());
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    overlay.set_value(input, 30.).unwrap();
    let mut bytes = poirot::eval::artifact::SnapshotSaveBuilder::checkpoint(overlay.snapshot())
        .to_buffer()
        .unwrap();
    bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&bytes)
            .is_err()
    );
    let restored = poirot::eval::artifact::SnapshotLoadBuilder::default()
        .from_bytes(&bytes)
        .unwrap();
    let artifact::RestoredCheckpoint::Source { workbook, status } =
        restored.into_checkpoint().unwrap()
    else {
        panic!("incompatible state was reused")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedIncompatible);
    assert_eq!(workbook.file_metadata, Some(metadata()));
    assert_eq!(
        workbook.sheet(0).unwrap().get(input.address).unwrap().value,
        CellValue::Number(30.)
    );
    let mut evaluation = Evaluation::with_workbook(workbook)
        .with_policy(EvaluationPolicy::default())
        .build(context())
        .unwrap();
    let output = evaluation.target("Inputs!B1").unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(60.));
}

#[test]
fn direct_restore_reports_preparation_status_and_accepts_explicit_recovery_policy() {
    let book = workbook();
    let mut evaluation = Evaluation::with_workbook(book.model())
        .build(context())
        .unwrap();
    let input = evaluation.target("Inputs!A1").unwrap();
    evaluation.set_value(input, 30.).unwrap();
    evaluation.compile_all();
    let bytes = poirot::eval::artifact::SnapshotSaveBuilder::checkpoint(evaluation.snapshot())
        .to_buffer()
        .unwrap();
    let mut restored = poirot::eval::artifact::SnapshotLoadBuilder::default()
        .from_bytes(&bytes)
        .unwrap()
        .into_evaluation(context(), EvaluationPolicy::default())
        .unwrap();
    assert_eq!(restored.prepared_status, artifact::PreparedStatus::Restored);
    let output = restored.evaluation.target("Inputs!B1").unwrap();
    assert_eq!(
        restored.evaluation.cell(output).unwrap(),
        CellValue::Number(60.)
    );
    let mut recovered = poirot::eval::artifact::SnapshotLoadBuilder::default()
        .prepared_policy(artifact::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap()
        .into_evaluation(context(), EvaluationPolicy::default())
        .unwrap();
    assert_eq!(
        recovered.prepared_status,
        artifact::PreparedStatus::DiscardedByRequest
    );
    assert_eq!(
        recovered.evaluation.cell(output).unwrap(),
        CellValue::Number(60.)
    );
}

#[test]
fn metadata_changes_do_not_invalidate_prepared_source_identity() {
    let book = workbook();
    let mut overlay = Overlay::with_workbook(book.model()).build().unwrap();
    let output = overlay.target("Inputs!B1").unwrap();
    let context = context();
    overlay.evaluation(&context).unwrap().cell(output).unwrap();
    let (_, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut changed = book.clone();
    changed.model_mut().file_metadata = Some(metadata());
    assert!(book.model().snapshot().matches(changed.model()));
    let mut attached = state.attach(changed).unwrap();
    let mut evaluation = attached.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(42.));
    assert_eq!(evaluation.usage().formula_executions, 0);
}

#[test]
fn owned_facade_flush_and_shared_model_handles_remain_explicit() {
    let mut overlay = Overlay::with_workbook(workbook()).build().unwrap();
    let input = overlay.target("Inputs!A1").unwrap();
    overlay.set_value(input, 30.).unwrap();
    overlay.flush().unwrap();
    assert_eq!(
        overlay
            .workbook()
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .value,
        CellValue::Number(30.)
    );
    let shared = std::sync::Arc::new(workbook().into_model());
    let mut evaluation = Evaluation::with_workbook(shared.clone())
        .build(context())
        .unwrap();
    let output = evaluation.target("Inputs!B1").unwrap();
    assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(42.));
    assert_eq!(
        shared.sheet(0).unwrap().get(output.address).unwrap().value,
        CellValue::Number(-1.)
    );
}

#[test]
fn load_options_replace_all_fields_and_later_setters_override_their_field() {
    let bytes = include_bytes!("../crates/poirot-io/tests/fixtures/poi-StringFormulas.xls");
    let small = io::ResourceBudget {
        max_records: 0,
        ..Default::default()
    };
    let options = io::ReadOptions {
        xls_budget: small,
        ..Default::default()
    };
    assert!(
        Workbook::load()
            .xls_budget(small)
            .options(io::ReadOptions::default())
            .from_bytes(bytes)
            .is_ok()
    );
    assert!(
        Workbook::load()
            .options(options)
            .xls_budget(io::ResourceBudget::default())
            .from_bytes(bytes)
            .is_ok()
    );
    assert!(matches!(
        Workbook::load()
            .options(io::ReadOptions::default())
            .xls_budget(small)
            .from_bytes(bytes),
        Err(poirot::files::WorkbookError::ImportOrXlsx(
            io::IoError::BiffPreflight(io::PreflightError::Resource { .. })
        ))
    ));
}

#[test]
fn loading_a_missing_file_preserves_the_filesystem_error_and_source() {
    let directory = TestDirectory::new();
    let path = directory.0.join("missing.xlsx");
    assert!(matches!(
        io::read_workbook::<poirot::model::Workbook>(&path),
        Err(io::IoError::File(error)) if error.kind() == std::io::ErrorKind::NotFound
    ));
    let error = Workbook::open(path).unwrap_err();
    assert!(matches!(&error, poirot::files::WorkbookError::File(source)
        if source.kind() == std::io::ErrorKind::NotFound));
    let source = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
}
