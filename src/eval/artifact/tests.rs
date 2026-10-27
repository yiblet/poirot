use super::bounded;
use super::codec::{
    HEADER, SOURCE_CHECKSUM, decode, deserialize, encode, encode_workbook, encode_workspace,
    read_file, sections,
};
use super::prepared_policy::PreparedPolicy;
use super::prepared_status::PreparedStatus;
use super::restored_artifact::RestoredArtifact;
use super::snapshot_error::SnapshotError;
use super::snapshot_limits::SnapshotLimits;
use super::snapshot_load_builder::SnapshotLoadBuilder;
use crate::eval;
use crate::eval::config::EvaluationContext;
use crate::eval::overlay::{Evaluation, Overlay};
use crate::model::{CellValue, Workbook};
use poirot_io as io;

fn workbook() -> Workbook {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 3.).unwrap();
    sheet
        .set_formula_with_cached_value("B1", "A1*2", CellValue::Number(999.))
        .unwrap();
    sheet
        .set_formula_with_cached_value("C1", "A1*3", CellValue::Number(999.))
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
fn strict_restore_requires_present_preparation_at_every_decode_entry() {
    let bytes = encode_workbook(&workbook(), SnapshotLimits::default()).unwrap();
    assert!(matches!(
        decode(
            &bytes,
            SnapshotLimits::default(),
            PreparedPolicy::RequireCompatible
        ),
        Err(SnapshotError::Envelope(
            crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::CompatiblePreparedStateRequiredButAbsent
        ))
    ));
    assert!(
        SnapshotLoadBuilder::default()
            .prepared_policy(PreparedPolicy::RequireCompatible)
            .from_bytes(&bytes)
            .is_err()
    );
    let restored = decode_default(&bytes).unwrap();
    assert_eq!(restored.prepared_status, PreparedStatus::Absent);
    assert_eq!(restored.workbook, workbook());
}

#[test]
fn source_file_rehydrates_evaluates_and_writes_xlsx_with_diagnostics() {
    let limits = SnapshotLimits::default();
    let mut book = workbook();
    let metadata = crate::model::FileMetadata {
        format: crate::model::FileFormat::Xlsx,
        import_warnings: vec![crate::model::ImportDiagnostic {
            kind: crate::model::ImportDiagnosticKind::OmittedFeatures,
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
    let reopened = io::read_workbook::<crate::model::Workbook>(&path).unwrap();
    let s = reopened.sheet(0).unwrap();
    assert_eq!(s.get("B1").unwrap().unwrap().value, CellValue::Number(6.));
    assert_eq!(s.get("C1").unwrap().unwrap().value, CellValue::Number(9.));
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
            crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::IncompatiblePreparedEngineRevision
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
        Err(SnapshotError::Envelope(crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::UnsupportedFileVersion))
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
        Err(SnapshotError::Envelope(crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::TrailingSectionBytes))
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
    other.sheet_mut(0).unwrap().set_value("A1", 99.).unwrap();
    let bad = encode(&other, prepared, revision, SnapshotLimits::default()).unwrap();
    assert!(matches!(
        decode_default(&bad),
        Err(SnapshotError::Envelope(
            super::snapshot_envelope_error::SnapshotEnvelopeError::SourceAndPreparedStateDisagree
        ))
    ));
}

#[test]
fn provider_implementations_remain_external_and_identity_checked() {
    use crate::eval::{functions::extensions::*, functions::registry::Arity};
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_formula("B1", "PROVIDER()")
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
            "A2",
            CellValue::Number(f64::from_bits(0x7ff8_0000_0000_1234)),
        )
        .unwrap();
    sheet.set_value("A3", CellValue::Number(-0.)).unwrap();
    sheet
        .set_value(
            "A4",
            CellValue::TextUtf16(crate::model::ExcelText::from_units(vec![0xd800, 65])),
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
        .set_formula("D1", "SQRT(-1)")
        .unwrap();
    let bytes = encode_workbook(&book, SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    let mut context = EvaluationContext::default();
    context.limits = context.limits.max_work(0);
    let mut run = Evaluation::with_workbook(&restored.workbook)
        .build(context)
        .unwrap();
    let target = run.target("S!B1").unwrap();
    assert!(matches!(
        run.cell(target),
        Err(eval::EvaluationError::Budget(_))
    ));
    assert!(run.changes().is_empty());
    let mut run = Evaluation::with_workbook(&restored.workbook)
        .build(EvaluationContext::default())
        .unwrap();
    let target = run.target("S!D1").unwrap();
    assert_eq!(
        run.cell(target).unwrap(),
        CellValue::Error(crate::model::CellError::Num)
    );
}

#[test]
fn older_source_schemas_are_rejected_before_decoding() {
    let workbook = Workbook::new();
    let mut bytes = encode_workbook(&workbook, SnapshotLimits::default()).unwrap();
    for schema in 0..crate::model::snapshot::SCHEMA_VERSION {
        bytes[10..12].copy_from_slice(&schema.to_le_bytes());
        assert!(matches!(
            decode_default(&bytes),
            Err(SnapshotError::Envelope(crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::UnsupportedSourceSchema))
        ));
    }
}

#[test]
fn presentation_roundtrip_and_prepared_identity_are_complete() {
    let mut book = workbook();
    let mut p = crate::model::SheetPresentation {
        visibility: crate::model::SheetVisibility::VeryHidden,
        tab_color: Some([1, 127, 255]),
    };
    book.sheet_mut(0).unwrap().set_presentation(p);
    let bytes = encode_workbook(&book, Default::default()).unwrap();
    assert_eq!(
        &bytes[10..12],
        &crate::model::snapshot::SCHEMA_VERSION.to_le_bytes()
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
    let bytes =
        super::snapshot_save_builder::SnapshotSaveBuilder::checkpoint(evaluation.snapshot())
            .to_buffer()
            .unwrap();
    let (_, prepared, revision) = sections(&bytes, usize::MAX).unwrap();
    let mut changed = workbook();
    changed.file_metadata = Some(crate::model::FileMetadata {
        format: crate::model::FileFormat::Xlsx,
        import_warnings: vec![crate::model::ImportDiagnostic {
            kind: crate::model::ImportDiagnosticKind::ExternalLink,
            detail: "omitted external relationship".into(),
        }],
    });
    assert!(book.snapshot().matches(&changed));
    let inconsistent = encode(&changed, prepared, revision, Default::default()).unwrap();
    assert!(matches!(
        decode_default(&inconsistent),
        Err(SnapshotError::Envelope(
            super::snapshot_envelope_error::SnapshotEnvelopeError::SourceAndPreparedStateDisagree
        ))
    ));
}

#[test]
fn native_visual_style_context_and_sparse_edits_obey_payload_quotas() {
    use crate::model::{CellStyle, FontStyle, StyleContext, VisualStyle};
    use std::sync::Arc;
    let style = CellStyle {
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                name: Some("authored family".repeat(20).into()),
                bold: true,
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut sparse = Overlay::with_workbook(workbook()).build().unwrap();
    sparse
        .set_style(
            crate::eval::coordinates::EvaluationTarget {
                sheet: 0,
                address: "E1".parse().unwrap(),
            },
            style.clone(),
        )
        .unwrap();
    let sparse_bytes = encode_workspace(sparse.snapshot(), Default::default()).unwrap();
    let sparse_limits = SnapshotLimits {
        max_decoded_bytes: style.text_bytes().unwrap() + 100,
        ..Default::default()
    };
    assert!(decode(&sparse_bytes, sparse_limits, PreparedPolicy::SourceOnly).is_ok());
    assert!(
        decode(
            &sparse_bytes,
            sparse_limits,
            PreparedPolicy::ReuseCompatible
        )
        .is_err()
    );
    let mut book = workbook();
    book.style_context = StyleContext::new(Some(vec![b'x'; 2048]), vec![[255, 1, 2, 3]])
        .unwrap()
        .with_default_style(style.clone());
    book.sheet_mut(0)
        .unwrap()
        .set_style("D1", style.clone())
        .unwrap();
    let bytes = encode_workbook(&book, Default::default()).unwrap();
    assert_eq!(decode_default(&bytes).unwrap().workbook, book);
    assert!(
        decode(
            &bytes,
            SnapshotLimits {
                max_decoded_bytes: 2047,
                ..Default::default()
            },
            PreparedPolicy::ReuseCompatible
        )
        .is_err()
    );
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    overlay
        .set_style(
            crate::eval::coordinates::EvaluationTarget {
                sheet: 0,
                address: "E1".parse().unwrap(),
            },
            style.clone(),
        )
        .unwrap();
    let bytes = encode_workspace(overlay.snapshot(), Default::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    let effective = restored
        .workspace
        .unwrap()
        .source_workbook(&restored.workbook)
        .unwrap();
    assert_eq!(
        effective
            .sheet(0)
            .unwrap()
            .get("E1")
            .unwrap()
            .unwrap()
            .style,
        style
    );
    assert!(
        decode(
            &bytes,
            SnapshotLimits {
                max_decoded_bytes: 4096,
                ..Default::default()
            },
            PreparedPolicy::ReuseCompatible
        )
        .is_err()
    );
    assert!(
        encode_workspace(
            overlay.snapshot(),
            SnapshotLimits {
                max_file_bytes: bytes.len() - 1,
                ..Default::default()
            }
        )
        .is_err()
    );
}

/// Two styled sheets with formulas, a spill, a defined name and import metadata.
fn styled_workbook() -> Workbook {
    use crate::model::{CellStyle, FontStyle, VisualStyle};
    use std::sync::Arc;
    let bold = CellStyle {
        visual: Some(Arc::new(VisualStyle {
            font: Some(FontStyle {
                bold: true,
                ..Default::default()
            }),
            ..Default::default()
        })),
        ..Default::default()
    };
    let mut book = workbook();
    book.file_metadata = Some(crate::model::FileMetadata {
        format: crate::model::FileFormat::Xlsx,
        import_warnings: Vec::new(),
    });
    book.define_name("Rate", "S!$A$1", None).unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_style("A1", bold.clone()).unwrap();
    sheet.set_value("A2", "text").unwrap();
    sheet.set_style("A2", bold.clone()).unwrap();
    sheet
        .set_formula_with_cached_value("D1", "SEQUENCE(3)", CellValue::Blank)
        .unwrap();
    let other = book.add_sheet("T").unwrap();
    other.set_value("A1", 5.).unwrap();
    other
        .set_formula_with_cached_value("B1", "S!B1+A1", CellValue::Blank)
        .unwrap();
    other.set_style("B1", bold).unwrap();
    other.set_formula("C1", "Rate*10").unwrap();
    book
}

/// Cells whose values the checkpoint restore tests compare.
const CHECKED_CELLS: [&str; 9] = [
    "S!A1", "S!B1", "S!C1", "S!D1", "S!D2", "S!D3", "S!E5", "T!B1", "T!C1",
];

/// Evaluate [`CHECKED_CELLS`], returning their values and the formula executions it took.
fn checked_values(overlay: &mut Overlay<'_, Workbook>) -> (Vec<CellValue>, u64) {
    let targets = CHECKED_CELLS.map(|cell| overlay.target(cell).unwrap());
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let values = targets.map(|target| evaluation.cell(target).unwrap());
    (values.to_vec(), evaluation.usage().formula_executions)
}

/// Save `overlay` as a checkpoint, restore it, and require the same baseline, authored
/// edits, retained results and values. Returns the file bytes.
fn assert_checkpoint_round_trip(overlay: &mut Overlay<'_, Workbook>) -> Vec<u8> {
    let bytes = encode_workspace(overlay.snapshot(), SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    assert_eq!(restored.prepared_status, PreparedStatus::Restored);
    assert_eq!(restored.workbook(), overlay.workbook());
    let super::restored_checkpoint::RestoredCheckpoint::Prepared(mut restored) =
        restored.into_checkpoint().unwrap()
    else {
        panic!("checkpoint preparation was not restored");
    };
    assert_eq!(restored.cached_cells(), overlay.cached_cells());
    assert_eq!(restored.changes(), overlay.changes());
    let retained = overlay.cached_cells() > 0;
    let (expected, _) = checked_values(overlay);
    let (values, executions) = checked_values(&mut restored);
    assert_eq!(values, expected);
    if retained {
        assert_eq!(executions, 0);
    }
    bytes
}

#[test]
fn checkpoints_store_the_workbook_once_and_restore_identical_results() {
    use crate::model::CellStyle;
    let number = CellValue::Number;
    let mut overlay = Overlay::with_workbook(styled_workbook()).build().unwrap();
    overlay.compile_all();
    let detached = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let compiled = assert_checkpoint_round_trip(&mut overlay);
    // Unedited preparation omits the model that the detached snapshot stores, so only
    // the variant tag and the trailing source checksum differ in size.
    let (source, prepared, _) = sections(&compiled, usize::MAX).unwrap();
    assert_eq!(
        prepared.len() - SOURCE_CHECKSUM + source.len(),
        detached.len()
    );
    assert_eq!(
        checked_values(&mut overlay).0,
        [
            number(3.),
            number(6.),
            number(9.),
            number(1.),
            number(2.),
            number(3.),
            CellValue::Blank,
            number(11.),
            number(30.),
        ]
    );
    let warm = assert_checkpoint_round_trip(&mut overlay);
    assert!(overlay.cached_cells() > 0);
    assert_ne!(warm, compiled);

    let target =
        |overlay: &Overlay<'_, Workbook>, reference: &str| overlay.target(reference).unwrap();
    let (a1, a2, c1, e5, t_a1, t_b1) = (
        target(&overlay, "S!A1"),
        target(&overlay, "S!A2"),
        target(&overlay, "S!C1"),
        target(&overlay, "S!E5"),
        target(&overlay, "T!A1"),
        target(&overlay, "T!B1"),
    );
    overlay.set_value(a1, 7.).unwrap();
    overlay.remove_cell(c1).unwrap();
    overlay.set_formula(e5, "A1+T!A1").unwrap();
    overlay
        .set_style(
            a2,
            CellStyle {
                number_format: Some("0.00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    overlay.set_style(t_a1, CellStyle::default()).unwrap();
    overlay.remove_cell(t_b1).unwrap();
    overlay.compile_all();
    let edited = encode_workspace(overlay.snapshot(), SnapshotLimits::default()).unwrap();
    // Edited preparation keeps its baseline beside the effective source.
    let (_, prepared, _) = sections(&edited, usize::MAX).unwrap();
    assert_eq!(
        prepared.len() - SOURCE_CHECKSUM,
        postcard::to_allocvec(&overlay.snapshot()).unwrap().len()
    );
    assert_eq!(
        checked_values(&mut overlay).0,
        [
            number(7.),
            number(14.),
            CellValue::Blank,
            number(1.),
            number(2.),
            number(3.),
            number(12.),
            CellValue::Blank,
            number(70.),
        ]
    );
    let warm_edited = assert_checkpoint_round_trip(&mut overlay);
    assert!(overlay.cached_cells() > 0);
    overlay.clear_calculated_cache();
    assert_checkpoint_round_trip(&mut overlay);

    let source = decode(
        &warm_edited,
        SnapshotLimits::default(),
        PreparedPolicy::SourceOnly,
    )
    .unwrap()
    .into_parts()
    .0;
    let sheet = source.sheet(0).unwrap();
    assert_eq!(sheet.get("A1").unwrap().unwrap().value, number(7.));
    assert!(sheet.get("C1").unwrap().is_none());
    assert_eq!(
        sheet.get("E5").unwrap().unwrap().formula.as_deref(),
        Some("A1+T!A1")
    );
    assert_eq!(
        sheet.get("A2").unwrap().unwrap().style.number_format,
        Some("0.00".into())
    );
    assert!(source.sheet(1).unwrap().get("B1").unwrap().is_none());
}

#[test]
fn owned_checkpoint_snapshots_restore_like_borrowed_ones() {
    let mut overlay = Overlay::with_workbook(styled_workbook()).build().unwrap();
    overlay
        .evaluation(&EvaluationContext::default())
        .unwrap()
        .all()
        .unwrap();
    let owned = overlay.snapshot().into_owned().unwrap();
    let bytes = encode_workspace(owned, SnapshotLimits::default()).unwrap();
    let restored = decode_default(&bytes).unwrap();
    assert_eq!(restored.workbook(), overlay.workbook());
    let super::restored_checkpoint::RestoredCheckpoint::Prepared(mut restored) =
        restored.into_checkpoint().unwrap()
    else {
        panic!("checkpoint preparation was not restored");
    };
    assert_eq!(restored.changes(), overlay.changes());
    assert_eq!(
        checked_values(&mut restored),
        (checked_values(&mut overlay).0, 0)
    );
}

#[test]
fn caller_supplied_baselines_are_still_compared_on_attach() {
    let mut overlay = Overlay::with_workbook(styled_workbook()).build().unwrap();
    overlay.compile_all();
    let bytes = encode_workspace(overlay.snapshot(), SnapshotLimits::default()).unwrap();
    let (workbook, state) = decode_default(&bytes).unwrap().into_parts();
    let mut other = workbook.clone();
    other.sheet_mut(0).unwrap().set_value("A1", 4.).unwrap();
    assert!(matches!(
        state.unwrap().attach(&other),
        Err(eval::overlay::OverlayError::Snapshot(
            crate::eval::overlay::OverlaySnapshotError::BaselineDiffersFromDetachedState
        ))
    ));
    let (workbook, state) = decode_default(&bytes).unwrap().into_parts();
    let state = state.unwrap();
    assert!(state.source_workbook(&other).is_err());
    let mut attached = state.attach(&workbook).unwrap();
    let target = attached.target("T!C1").unwrap();
    let context = EvaluationContext::default();
    let mut evaluation = attached.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(target).unwrap(), CellValue::Number(30.));
}

#[test]
fn prepared_sections_must_end_with_their_source_checksum() {
    let mut overlay = Overlay::with_workbook(styled_workbook()).build().unwrap();
    overlay.compile_all();
    let bytes = encode_workspace(overlay.snapshot(), SnapshotLimits::default()).unwrap();
    let (_, prepared, revision) = sections(&bytes, usize::MAX).unwrap();
    for short in [&prepared[..1], &prepared[..SOURCE_CHECKSUM - 1]] {
        let truncated = encode(&styled_workbook(), short, revision, Default::default()).unwrap();
        assert!(matches!(
            decode_default(&truncated),
            Err(SnapshotError::Envelope(crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::TruncatedPreparedSection))
        ));
    }
    let mut bad_checksum = bytes.clone();
    *bad_checksum.last_mut().unwrap() ^= 1;
    assert!(matches!(
        decode_default(&bad_checksum),
        Err(SnapshotError::Envelope(
            super::snapshot_envelope_error::SnapshotEnvelopeError::SourceAndPreparedStateDisagree
        ))
    ));
    // Dropping a payload byte keeps the checksum but leaves a malformed payload.
    let mut short_payload = prepared[..prepared.len() - SOURCE_CHECKSUM - 1].to_vec();
    short_payload.extend_from_slice(&prepared[prepared.len() - SOURCE_CHECKSUM..]);
    let malformed = encode(
        &styled_workbook(),
        &short_payload,
        revision,
        Default::default(),
    )
    .unwrap();
    assert!(decode_default(&malformed).is_err());
    assert_eq!(
        decode(
            &malformed,
            SnapshotLimits::default(),
            PreparedPolicy::SourceOnly
        )
        .unwrap()
        .workbook(),
        &styled_workbook()
    );
}

/// Source-only file written by the previous prepared format (COMPATIBILITY_REVISION 75);
/// its source section format is unchanged. Workbook: [`workbook`].
#[rustfmt::skip]
const OLD_SOURCE: &str = concat!(
    "524f49534e415000010008000000000000000000000000004c00000000000000000000000000000001015300",
    "0001000003000001808080808080808440000000000101808080808080cec740010441312a32000000020180",
    "8080808080cec740010441312a330000000000000001000000000000",
);

/// Checkpoint written by the previous prepared format (COMPATIBILITY_REVISION 75) with its
/// model stored twice: [`workbook`] compiled with `S!A1` edited to 7.
#[rustfmt::skip]
const OLD_CHECKPOINT: &str = concat!(
    "524f49534e41500001000800010000004b000000000000004c00000000000000b30100000000000001015300",
    "0001000003000001808080808080808e40000000000101808080808080cec740010441312a32000000020180",
    "8080808080cec740010441312a3300000000000000010000000000004d010153000001000003000001808080",
    "808080808440000000000101808080808080cec740010441312a320000000201808080808080cec740010441",
    "312a330000000000000001000000000000800880018080408080010101000200800880018080408080011500",
    "020300120000000001010101ff0400010110001f000001010101e30400030402000000000101010180040000",
    "0000090000000000000000000100000700000000000100000000000000000100000200000080808080040001",
    "0000000000150000020000010000000000000100000300120000000001010101ff0400010110001f00000101",
    "0101e30400030402000000000101010180040000000009000000000000000000010000070000000000010000",
    "000000000000010000020000008080a080040001000000000015000002000001000000000000010000020441",
    "312a320000010000141400000000000000000000000000000000000000000100000000000000000000000441",
    "312a330001010000141400000000000000000000000000000000000000000100000000000000000000000100",
    "000001808080808080808e400000000000000000000000",
);

/// Unedited compiled checkpoint of [`workbook`] written by prepared payload version 78
/// (COMPATIBILITY_REVISION 76), whose compiler cache entries carry no template keys.
#[rustfmt::skip]
const REVISION_76_CHECKPOINT: &str = concat!(
    "524f49534e41500001000800010000004c000000000000004c000000000000005b0100000000000001015300",
    "0001000003000001808080808080808440000000000101808080808080cec740010441312a32000000020180",
    "8080808080cec740010441312a3300000000000000010000000000004e018008800180804080800101010002",
    "00800880018080408080011500020300120000000001010101ff0400010110001f000001010101e304000304",
    "0200000000010101018004000000000900000000000000000001000007000000000001000000000000000001",
    "0000020000008080808004000100000000001500000200000100000000000001000003001200000000010101",
    "01ff0400010110001f000001010101e304000304020000000001010101800400000000090000000000000000",
    "00010000070000000000010000000000000000010000020000008080a0800400010000000000150000020000",
    "01000000000000010000020441312a3200000100001414000000000000000000000000000000000000000001",
    "00000000000000000000000441312a3300010100001414000000000000000000000000000000000000000001",
    "00000000000000000000000000000000000000f05d05da",
);

/// Unedited compiled checkpoint of [`workbook`] written by prepared payload version 79
/// (COMPATIBILITY_REVISION 77), whose template keys came from std's unspecified
/// `DefaultHasher` rather than the fixed `normalized_hash` algorithm.
#[rustfmt::skip]
const REVISION_77_CHECKPOINT: &str = concat!(
    "524f49534e41500001000800010000004d000000000000004c00000000000000710100000000000001015300",
    "0001000003000001808080808080808440000000000101808080808080cec740010441312a32000000020180",
    "8080808080cec740010441312a3300000000000000010000000000004f018008800180804080800101010002",
    "00800880018080408080011500020300120000000001010101ff0400010110001f000001010101e304000304",
    "0200000000010101018004000000000900000000000000000001000007000000000001000000000000000001",
    "0000020000008080808004000100000000001500000200000100000000000001000003001200000000010101",
    "01ff0400010110001f000001010101e304000304020000000001010101800400000000090000000000000000",
    "00010000070000000000010000000000000000010000020000008080a0800400010000000000150000020000",
    "01000000000000010000020441312a3200000100001414000000000000000000000000000000000000000001",
    "000000000000000000000001d7d1e98eaccfecbcf1010441312a330001010000141400000000000000000000",
    "0000000000000000000001000000000000000000000001e9f8dee0fbf2d9e5b3010000000000000000f05d05",
    "da",
);

/// Decode a hex fixture.
fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

#[test]
fn previous_format_prepared_sections_are_rejected_while_their_source_restores() {
    let source = unhex(OLD_SOURCE);
    let restored = decode_default(&source).unwrap();
    assert_eq!(restored.prepared_status, PreparedStatus::Absent);
    assert_eq!(restored.workbook(), &workbook());
    assert_eq!(
        source,
        encode_workbook(&workbook(), SnapshotLimits::default()).unwrap()
    );

    let checkpoint = unhex(OLD_CHECKPOINT);
    assert_eq!(&checkpoint[16..24], &75_u64.to_le_bytes());
    assert!(matches!(
        decode(
            &checkpoint,
            SnapshotLimits::default(),
            PreparedPolicy::RequireCompatible
        ),
        Err(SnapshotError::Envelope(
            crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::IncompatiblePreparedEngineRevision
        ))
    ));
    let mut edited = workbook();
    edited.sheet_mut(0).unwrap().set_value("A1", 7.).unwrap();
    for policy in [PreparedPolicy::ReuseCompatible, PreparedPolicy::SourceOnly] {
        let restored = decode(&checkpoint, SnapshotLimits::default(), policy).unwrap();
        assert_ne!(restored.prepared_status, PreparedStatus::Restored);
        assert_eq!(restored.workbook(), &edited);
    }
    // Even under the current revision, the old payload is refused rather than misread.
    let mut relabeled = checkpoint;
    relabeled[16..24]
        .copy_from_slice(&crate::eval::overlay::snapshot::COMPATIBILITY_REVISION.to_le_bytes());
    assert!(decode_default(&relabeled).is_err());
}

#[test]
fn compiler_caches_with_older_template_keys_are_rejected_while_their_source_restores() {
    for (fixture, revision) in [
        (REVISION_76_CHECKPOINT, 76_u64),
        (REVISION_77_CHECKPOINT, 77),
    ] {
        let checkpoint = unhex(fixture);
        assert_eq!(&checkpoint[16..24], &revision.to_le_bytes());
        assert!(matches!(
            decode(
                &checkpoint,
                SnapshotLimits::default(),
                PreparedPolicy::RequireCompatible
            ),
            Err(SnapshotError::Envelope(
                crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::IncompatiblePreparedEngineRevision
            ))
        ));
        let restored = decode_default(&checkpoint).unwrap();
        assert_eq!(
            restored.prepared_status,
            PreparedStatus::DiscardedIncompatible
        );
        assert_eq!(restored.workbook(), &workbook());
        // Relabeled as current, the old payload is refused rather than misread.
        let mut relabeled = checkpoint;
        relabeled[16..24]
            .copy_from_slice(&crate::eval::overlay::snapshot::COMPATIBILITY_REVISION.to_le_bytes());
        assert!(decode_default(&relabeled).is_err());
    }
}

#[test]
fn expanded_catalog_checkpoint_preserves_source_and_policy_dependencies() {
    use crate::eval::artifact::restored_checkpoint::RestoredCheckpoint;
    use crate::eval::config::ByteTextLanguage;

    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "1+1"),
        ("B1", "FORMULATEXT(A1)"),
        ("C1", "LENB(\"😀\")"),
        ("D1", "C1+1"),
        ("E1", "ENCODEURL(\"a b\")"),
    ] {
        sheet.set_formula(cell, formula).unwrap();
    }
    let context = EvaluationContext::default();
    let mut original = Overlay::with_workbook(&book).build().unwrap();
    let source = original.target("S!A1").unwrap();
    let metadata = original.target("S!B1").unwrap();
    let bytes_target = original.target("S!C1").unwrap();
    let dependent = original.target("S!D1").unwrap();
    let url = original.target("S!E1").unwrap();
    {
        let mut run = original.evaluation(&context).unwrap();
        assert_eq!(run.cell(metadata).unwrap(), CellValue::Text("=1+1".into()));
        assert_eq!(run.cell(dependent).unwrap(), CellValue::Number(3.));
        assert_eq!(run.cell(url).unwrap(), CellValue::Text("a%20b".into()));
    }
    let mut bytes = encode_workspace(original.snapshot(), SnapshotLimits::default()).unwrap();
    let restored = decode(
        &bytes,
        Default::default(),
        PreparedPolicy::RequireCompatible,
    )
    .unwrap();
    let RestoredCheckpoint::Prepared(mut restored) = restored.into_checkpoint().unwrap() else {
        panic!("expected compatible preparation")
    };
    {
        let mut run = restored.evaluation(&context).unwrap();
        assert_eq!(run.cell(metadata).unwrap(), CellValue::Text("=1+1".into()));
        assert_eq!(run.cell(dependent).unwrap(), CellValue::Number(3.));
        assert_eq!(run.cell(url).unwrap(), CellValue::Text("a%20b".into()));
        assert_eq!(run.usage().formula_executions, 0);
    }
    // The numeric result stays equal: FORMULATEXT depends on authored source.
    restored.set_formula(source, "2").unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(metadata)
            .unwrap(),
        CellValue::Text("=2".into())
    );
    let mut policy = restored.policy().clone();
    policy.byte_text_language = ByteTextLanguage::Dbcs;
    restored.set_policy(policy).unwrap();
    assert!(restored.cached_value(bytes_target).is_none());
    assert!(restored.cached_value(dependent).is_none());
    assert!(restored.cached_value(url).is_some());
    let dbcs = encode_workspace(restored.snapshot(), SnapshotLimits::default()).unwrap();
    let RestoredCheckpoint::Prepared(restored) =
        decode_default(&dbcs).unwrap().into_checkpoint().unwrap()
    else {
        panic!("expected compatible preparation")
    };
    assert_eq!(restored.policy().byte_text_language, ByteTextLanguage::Dbcs);

    // Revision 79 predates the expanded catalog. Its optional preparation must
    // never be decoded, even if the durable workbook source is still current.
    bytes[16..24].copy_from_slice(&79_u64.to_le_bytes());
    let split = HEADER + u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;
    bytes[split..].fill(255);
    let fallback = decode_default(&bytes).unwrap();
    assert_eq!(
        fallback.prepared_status,
        PreparedStatus::DiscardedIncompatible
    );
    assert_eq!(fallback.workbook, book);
    assert!(fallback.workspace.is_none());
    let mut run = Evaluation::with_workbook(&fallback.workbook)
        .build(context)
        .unwrap();
    assert_eq!(run.cell(dependent).unwrap(), CellValue::Number(3.));
    assert!(matches!(decode(&bytes, Default::default(), PreparedPolicy::RequireCompatible),
        Err(SnapshotError::Envelope(crate::eval::artifact::snapshot_envelope_error::SnapshotEnvelopeError::IncompatiblePreparedEngineRevision))));
}
