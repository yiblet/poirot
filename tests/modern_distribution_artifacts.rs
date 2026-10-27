//! Public saved-source recovery and XLSX modern_distribution execution contracts.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
#[test]
fn incompatible_modern_distribution_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 0.1).unwrap();
    sheet.set_formula("D1", "BETA.DIST(A1,1,1,TRUE)").unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 0.2).unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        0.2.into()
    );
    let mut bytes = poirot::eval::artifact::SnapshotSaveBuilder::checkpoint(overlay.snapshot())
        .to_buffer()
        .unwrap();
    let previous = poirot::eval::overlay::snapshot::COMPATIBILITY_REVISION - 1;
    bytes[16..24].copy_from_slice(&previous.to_le_bytes());
    assert!(
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&bytes)
            .is_err()
    );
    let artifact::RestoredCheckpoint::Source { workbook, status } =
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .from_bytes(&bytes)
            .unwrap()
            .into_checkpoint()
            .unwrap()
    else {
        panic!()
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedIncompatible);
    assert_eq!(
        workbook
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .unwrap()
            .value,
        0.20.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 0.2.into());
    let artifact::RestoredCheckpoint::Source { workbook, status } =
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .prepared_policy(artifact::PreparedPolicy::SourceOnly)
            .from_bytes(&bytes)
            .unwrap()
            .into_checkpoint()
            .unwrap()
    else {
        panic!("source-only reused prepared results")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedByRequest);
    assert_eq!(
        workbook
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .unwrap()
            .value,
        0.20.into()
    );
}
#[test]
fn stored_modern_distribution_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "F.DIST.RT(3,2,2)"),
        ("D1", "BETA.DIST({0.25;0.5},1,1,TRUE)"),
        ("G1", "BINOM.DIST({0;1},2,0.5,FALSE)"),
    ] {
        sheet.set_formula(cell, formula).unwrap();
    }
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let book = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!D1", CellValue::Number(0.25)),
        ("S!D2", 0.5.into()),
        ("S!G2", 0.5.into()),
        ("S!A1", 0.25.into()),
    ] {
        let target = direct.target(cell).unwrap();
        let (CellValue::Number(actual), CellValue::Number(expected)) =
            (direct.cell(target).unwrap(), expected)
        else {
            panic!()
        };
        assert!((actual - expected).abs() <= expected.abs() * 2e-12 + 1e-12);
    }
}
