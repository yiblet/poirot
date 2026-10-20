use super::*;
use crate as eval;
use crate::{CellValue, overlay::Evaluation, overlay::EvaluationContext};
use poirot_io as io;

fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 3.).unwrap();
    sheet
        .set_formula_with_cached_value("B1".parse().unwrap(), "A1*2", CellValue::Number(999.))
        .unwrap();
    sheet
        .set_formula_with_cached_value("C1".parse().unwrap(), "A1*3", CellValue::Number(999.))
        .unwrap();
    book
}
fn decode_default(bytes: &[u8]) -> Result<RestoredArtifact, SnapshotError> {
    decode(
        bytes,
        SnapshotLimits::default(),
        PreparedPolicy::ReuseCompatible,
    )
}

#[test]
fn source_file_rehydrates_evaluates_and_writes_xlsx_with_diagnostics() {
    let limits = SnapshotLimits::default();
    let mut book = workbook();
    let metadata = poirot_model::FileMetadata {
        format: poirot_model::FileFormat::Xlsx,
        import_warnings: vec![poirot_model::ImportDiagnostic {
            kind: poirot_model::ImportDiagnosticKind::OmittedFeatures,
            detail: "source feature omitted".into(),
        }],
    };
    book.file_metadata = Some(metadata.clone());
    let bytes = encode_workbook(&book, limits).unwrap();
    let dir = std::env::temp_dir().join(format!("poirot-artifact-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("source.poirot");
    std::fs::write(&path, &bytes).unwrap();
    let restored = read_file(&path, limits, PreparedPolicy::ReuseCompatible).unwrap();
    assert_eq!(restored.prepared_status, PreparedStatus::Absent);
    assert_eq!(restored.workbook.file_metadata, Some(metadata.clone()));
    assert!(restored.workspace.is_none());
    let mut book = restored.workbook;
    let changes = {
        let mut evaluation = Evaluation::with_workbook(&book)
            .build(EvaluationContext::default())
            .unwrap();
        evaluation.all().unwrap();
        evaluation.changes()
    };
    book.apply(changes).unwrap();
    assert_eq!(book.file_metadata, Some(metadata));
    let path = dir.join("output.xlsx");
    let bytes = io::plan_xlsx_export(&book, Default::default())
        .unwrap()
        .execute()
        .unwrap();
    std::fs::write(&path, bytes).unwrap();
    let reopened = io::read_workbook(&path).unwrap();
    let s = reopened.sheet(0).unwrap();
    assert_eq!(
        s.get("B1".parse().unwrap()).unwrap().value,
        CellValue::Number(6.)
    );
    assert_eq!(
        s.get("C1".parse().unwrap()).unwrap().value,
        CellValue::Number(9.)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn prepared_restore_reuses_programs_and_fallback_preserves_edits_and_deletions() {
    let book = workbook();
    let mut evaluation = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let input = evaluation.target("S!A1").unwrap();
    let removed = evaluation.target("S!C1").unwrap();
    evaluation.set_value(input, 7.).unwrap();
    evaluation.remove_cell(removed).unwrap();
    evaluation.compile_all();
    let mut bytes = encode_workspace(evaluation.snapshot(), SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    assert_eq!(restored.prepared_status, PreparedStatus::Restored);
    let mut run = restored
        .workspace
        .unwrap()
        .into_evaluation(&restored.workbook, EvaluationContext::default())
        .unwrap();
    let parses = run.compiled_usage().parse_attempts;
    let target = run.target("S!B1").unwrap();
    assert_eq!(run.cell(target).unwrap(), CellValue::Number(14.));
    assert_eq!(run.compiled_usage().parse_attempts, parses);
    assert!(run.source_cell(removed).is_none());
    drop(run);
    // Header mismatch must be handled without decoding the optional cache at all.
    bytes[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    let split = HEADER + u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;
    bytes[split..].fill(255);
    let restored = decode_default(&bytes).unwrap();
    assert_eq!(
        restored.prepared_status,
        PreparedStatus::DiscardedIncompatible
    );
    assert!(restored.workspace.is_none());
    let mut run = Evaluation::with_workbook(&restored.workbook)
        .build(EvaluationContext::default())
        .unwrap();
    let target = run.target("S!B1").unwrap();
    assert_eq!(run.cell(target).unwrap(), CellValue::Number(14.));
    assert!(run.source_cell(removed).is_none());
    assert!(matches!(
        decode(
            &bytes,
            SnapshotLimits::default(),
            PreparedPolicy::RequireCompatible
        ),
        Err(SnapshotError::Envelope(
            "incompatible prepared engine revision"
        ))
    ));
}

#[test]
fn explicit_source_only_does_not_decode_preparation() {
    let book = workbook();
    let evaluation = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let mut bytes = encode_workspace(evaluation.snapshot(), SnapshotLimits::default()).unwrap();
    let split = HEADER + u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;
    bytes[split..].fill(255);
    let source = decode(
        &bytes,
        SnapshotLimits::default(),
        PreparedPolicy::SourceOnly,
    )
    .unwrap();
    assert_eq!(source.prepared_status, PreparedStatus::DiscardedByRequest);
    assert!(decode_default(&bytes).is_err());
}

#[test]
fn envelope_rejects_kind_versions_lengths_and_trailing_bytes_before_decode() {
    let bytes = encode_workbook(&workbook(), SnapshotLimits::default()).unwrap();
    for (offset, replacement) in [
        (0, b'X'),
        (8, 255),
        (10, 255),
        (12, 2),
        (13, 1),
        (24, 255),
        (32, 1),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = replacement;
        assert!(
            matches!(decode_default(&bad), Err(SnapshotError::Envelope(_))),
            "offset {offset}"
        );
    }
    assert!(decode_default(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_default(&trailing).is_err());
    let mut bad = bytes.clone();
    bad[8] = 255;
    bad[HEADER..].fill(255);
    assert!(matches!(
        decode_default(&bad),
        Err(SnapshotError::Envelope("unsupported file version"))
    ));
}

#[test]
fn limits_reject_before_oversized_reservation_and_string_copy() {
    let bytes = encode_workbook(&workbook(), SnapshotLimits::default()).unwrap();
    for limits in [
        SnapshotLimits {
            max_file_bytes: bytes.len() - 1,
            ..Default::default()
        },
        SnapshotLimits {
            max_decoded_values: 2,
            ..Default::default()
        },
        SnapshotLimits {
            max_decoded_bytes: 1,
            ..Default::default()
        },
        SnapshotLimits {
            max_depth: 1,
            ..Default::default()
        },
    ] {
        assert!(decode(&bytes, limits, PreparedPolicy::ReuseCompatible).is_err());
    }
    assert!(
        encode_workbook(
            &workbook(),
            SnapshotLimits {
                max_file_bytes: HEADER,
                ..Default::default()
            }
        )
        .is_err()
    );
    // A 1-billion-element sequence claim is only a few varint bytes: reject its hint
    // before a Vec visitor can reserve memory. No elements need to be present.
    let malicious = postcard::to_allocvec(&1_000_000_000_usize).unwrap();
    let budget = bounded::Budget::new(100, 100, 16);
    assert!(deserialize::<Vec<u8>>(&malicious, &budget).is_err());
    // Catch trailing data inside a length-consistent source section as well.
    let mut bad = bytes;
    bad.push(0);
    let len = (bad.len() - HEADER) as u64;
    bad[24..32].copy_from_slice(&len.to_le_bytes());
    assert!(matches!(
        decode_default(&bad),
        Err(SnapshotError::Envelope("trailing section bytes"))
    ));
}

#[test]
fn prepared_source_mismatch_is_rejected() {
    let book = workbook();
    let evaluation = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let bytes = encode_workspace(evaluation.snapshot(), SnapshotLimits::default()).unwrap();
    let (_, prepared, revision) = sections(&bytes, usize::MAX).unwrap();
    let mut other = workbook();
    other
        .sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), 99.)
        .unwrap();
    let bad = encode(&other, prepared, revision, SnapshotLimits::default()).unwrap();
    assert!(matches!(
        decode_default(&bad),
        Err(SnapshotError::Envelope(
            "source and prepared state disagree"
        ))
    ));
}

#[test]
fn provider_implementations_remain_external_and_identity_checked() {
    use eval::{extensions::*, registry::Arity};
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1".parse().unwrap(), "PROVIDER()")
        .unwrap();
    let contract = ExtensionContract {
        arity: Arity {
            minimum: 0,
            maximum: 0,
        },
        evaluation: ExtensionEvaluationStrategy::Eager,
        capabilities: ExtensionCapabilities {
            references: false,
            arrays: false,
            context: false,
        },
        cacheability: ExtensionCacheability::Deterministic,
    };
    fn provider(_: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
        Ok(ExtensionOutput::Scalar(CellValue::Number(42.)))
    }
    let registry = |revision: &str| {
        let mut r = ExtensionRegistry::new();
        r.register(
            ExtensionNamespace::UserDefined,
            "PROVIDER",
            revision,
            contract,
            provider,
        )
        .unwrap();
        r
    };
    let first = registry("v1");
    let mut evaluation = Evaluation::with_workbook(&book)
        .with_extensions(&first)
        .build(EvaluationContext::default())
        .unwrap();
    evaluation.compile_all();
    let bytes = encode_workspace(evaluation.snapshot(), SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    assert!(matches!(
        restored.workspace.unwrap().attach(&restored.workbook),
        Err(eval::overlay::OverlayError::RegistryMismatch)
    ));
    let second = registry("v2");
    let restored = decode_default(&bytes).unwrap();
    assert!(matches!(
        restored
            .workspace
            .unwrap()
            .attach_with_extensions(&restored.workbook, &second),
        Err(eval::overlay::OverlayError::RegistryMismatch)
    ));
    let restored = decode_default(&bytes).unwrap();
    let mut run = restored
        .workspace
        .unwrap()
        .into_evaluation_with_extensions(&restored.workbook, EvaluationContext::default(), &first)
        .unwrap();
    let target = run.target("S!B1").unwrap();
    assert_eq!(run.cell(target).unwrap(), CellValue::Number(42.));
}

#[test]
fn source_files_preserve_numeric_bits_and_exact_utf16() {
    let mut book = workbook();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_value(
            "A2".parse().unwrap(),
            CellValue::Number(f64::from_bits(0x7ff8_0000_0000_1234)),
        )
        .unwrap();
    sheet
        .set_value("A3".parse().unwrap(), CellValue::Number(-0.))
        .unwrap();
    sheet
        .set_value(
            "A4".parse().unwrap(),
            CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd800, 65])),
        )
        .unwrap();
    let bytes = encode_workbook(&book, SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    assert!(book.snapshot().matches(&restored.workbook));
}

#[test]
fn restored_execution_keeps_operational_failure_separate_from_spreadsheet_error() {
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("D1".parse().unwrap(), "SQRT(-1)")
        .unwrap();
    let bytes = encode_workbook(&book, SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    let mut context = EvaluationContext::default();
    context.budgets.run_work = 0;
    let mut run = Evaluation::with_workbook(&restored.workbook)
        .build(context)
        .unwrap();
    let target = run.target("S!B1").unwrap();
    assert!(matches!(
        run.cell(target),
        Err(eval::EvaluationFailure::Budget(_))
    ));
    assert!(run.changes().is_empty());
    let mut run = Evaluation::with_workbook(&restored.workbook)
        .build(EvaluationContext::default())
        .unwrap();
    let target = run.target("S!D1").unwrap();
    assert_eq!(
        run.cell(target).unwrap(),
        CellValue::Error(crate::CellError::Num)
    );
}

#[test]
fn older_source_schemas_are_rejected_before_decoding() {
    let workbook = Workbook::new();
    let mut bytes = encode_workbook(&workbook, SnapshotLimits::default()).unwrap();
    for schema in [0u16, 1, 2] {
        bytes[10..12].copy_from_slice(&schema.to_le_bytes());
        assert!(matches!(
            decode_default(&bytes),
            Err(SnapshotError::Envelope("unsupported source schema"))
        ));
    }
}

#[test]
fn presentation_roundtrip_and_prepared_identity_are_complete() {
    let mut book = workbook();
    let mut p = poirot_model::SheetPresentation {
        visibility: poirot_model::SheetVisibility::VeryHidden,
        tab_color: Some([1, 127, 255]),
    };
    book.sheet_mut(0).unwrap().set_presentation(p);
    let bytes = encode_workbook(&book, Default::default()).unwrap();
    assert_eq!(
        &bytes[10..12],
        &poirot_model::snapshot::SCHEMA_VERSION.to_le_bytes()
    );
    let restored = decode_default(&bytes).unwrap().workbook;
    assert!(restored.snapshot().matches(&book));
    p.tab_color = None;
    book.sheet_mut(0).unwrap().set_presentation(p);
    assert!(!restored.snapshot().matches(&book));
}

#[test]
fn durable_source_and_prepared_metadata_must_agree_even_without_calculation_identity() {
    let book = workbook();
    let evaluation = Evaluation::with_workbook(&book)
        .build(EvaluationContext::default())
        .unwrap();
    let bytes = evaluation.save_snapshot().to_buffer().unwrap();
    let (_, prepared, revision) = sections(&bytes, usize::MAX).unwrap();
    let mut changed = workbook();
    changed.file_metadata = Some(poirot_model::FileMetadata {
        format: poirot_model::FileFormat::Xlsx,
        import_warnings: vec![poirot_model::ImportDiagnostic {
            kind: poirot_model::ImportDiagnosticKind::ExternalLink,
            detail: "omitted external relationship".into(),
        }],
    });
    assert!(book.snapshot().matches(&changed));
    let inconsistent = encode(&changed, prepared, revision, Default::default()).unwrap();
    assert!(matches!(
        decode_default(&inconsistent),
        Err(SnapshotError::Envelope(
            "source and prepared state disagree"
        ))
    ));
}
