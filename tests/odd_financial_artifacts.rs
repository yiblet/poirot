//! Public saved-source recovery and XLSX odd_financial execution contracts.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
#[test]
fn incompatible_odd_financial_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 0.1).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "AMORLINC(1000,45000,45090,100,1,A1)")
        .unwrap();
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
        200.0.into()
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
        workbook.sheet(0).unwrap().get(input.address).unwrap().value,
        0.20.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 200.0.into());
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
        workbook.sheet(0).unwrap().get(input.address).unwrap().value,
        0.20.into()
    );
}
#[test]
fn stored_odd_financial_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        (
            "A1",
            "ODDLYIELD(DATE(2024,4,1),DATE(2024,10,1),DATE(2024,1,1),0.1,4195/42,100,2)",
        ),
        ("D1", "AMORLINC({1000;2000},45000,45090,100,1,0.2)"),
        (
            "G1",
            "ODDLPRICE(DATE(2024,4,1),DATE(2024,10,1),DATE(2024,1,1),{0.1;0.2},0,100,2)",
        ),
    ] {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let book = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!D1", CellValue::Number(200.0)),
        ("S!D2", 400.0.into()),
        ("S!G2", 110.0.into()),
        ("S!A1", 0.1.into()),
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
