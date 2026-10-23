//! Public saved-source recovery and XLSX engineering execution contracts.
use poirot::{CellValue, EvaluationContext, Overlay, Workbook, artifact};
#[test]
fn incompatible_radix_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 8.).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "DEC2OCT(A1)")
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
        "100".into()
    );
    let mut bytes = overlay.save_snapshot().to_buffer().unwrap();
    let previous = poirot::eval::overlay::snapshot::COMPATIBILITY_REVISION - 1;
    bytes[16..24].copy_from_slice(&previous.to_le_bytes());
    assert!(
        Workbook::load_snapshot()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&bytes)
            .is_err()
    );
    let artifact::RestoredCheckpoint::Source { workbook, status } = Workbook::load_snapshot()
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
    let mut direct = poirot::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), "100".into());
}
#[test]
fn stored_engineering_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "BIN2HEX({1;10})"),
        ("D1", "BITOR({8;16},1)"),
        ("G1", "CONVERT({1;2},\"byte\",\"bit\")"),
    ] {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = poirot::io::write_xlsx_to_buffer(&book).unwrap();
    let book = poirot::io::read_workbook_bytes(&bytes).unwrap();
    let mut direct = poirot::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!A2", CellValue::Text("2".into())),
        ("S!D2", 17.0.into()),
        ("S!G2", 16.0.into()),
    ] {
        let target = direct.target(cell).unwrap();
        assert_eq!(direct.cell(target).unwrap(), expected);
    }
}
