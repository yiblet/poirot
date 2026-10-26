//! Public saved-source recovery and XLSX engineering execution contracts.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
#[test]
fn incompatible_special_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 8.).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "GESTEP(A1,32)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 64.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        1.0.into()
    );
    let mut bytes = overlay.save_snapshot().to_buffer().unwrap();
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
        64.0.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 1.0.into());
}
#[test]
fn stored_engineering_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "BESSELI({0;1},0)"),
        ("D1", "GESTEP({-1;0})"),
        ("G1", "ERF.PRECISE({0;0})"),
    ] {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let book = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!A1", CellValue::Number(1.0)),
        ("S!D2", 1.0.into()),
        ("S!G2", 0.0.into()),
    ] {
        let target = direct.target(cell).unwrap();
        assert_eq!(direct.cell(target).unwrap(), expected);
    }
}
