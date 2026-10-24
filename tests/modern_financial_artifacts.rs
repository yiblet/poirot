//! Public saved-source recovery and XLSX financial execution contracts.
use poirot::{CellValue, EvaluationContext, Workbook, artifact, eval::overlay::Overlay};
#[test]
fn incompatible_financial_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 8.).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "SLN(A1,0,2)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 12.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        6.0.into()
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
        12.0.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 6.0.into());
    let artifact::RestoredCheckpoint::Source { workbook, status } = Workbook::load_snapshot()
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
        12.0.into()
    );
}
#[test]
fn stored_financial_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "RRI(2,100,121)"),
        ("D1", "SLN({1000;2000},100,3)"),
        ("G1", "XNPV({0;0.1},{-100;110},{1000;1365})"),
    ] {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = poirot::io::write_xlsx_to_buffer(&book).unwrap();
    let book = poirot::io::read_workbook_bytes(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!D1", CellValue::Number(300.0)),
        ("S!D2", (1900.0 / 3.0).into()),
        ("S!G1", 10.0.into()),
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
