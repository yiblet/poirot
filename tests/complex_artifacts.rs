//! Durable complex source/edit recovery and executable XLSX round trips.
use poirot::{CellValue, EvaluationContext, Workbook, artifact, eval::overlay::Overlay};
#[test]
fn old_complex_preparation_recovers_authored_source_and_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), "3+4j").unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "IMCONJUGATE(A1:A2)")
        .unwrap();
    sheet
        .set_formula("G1".parse().unwrap(), "IMSUM(A1:A2)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A2").unwrap();
    let output = overlay.target("S!G1").unwrap();
    overlay.set_value(input, "2j").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        "3+6j".into()
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
        panic!("old prepared cache accepted")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedIncompatible);
    assert_eq!(
        workbook.sheet(0).unwrap().get(input.address).unwrap().value,
        CellValue::Text("2j".into())
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(context)
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), "3+6j".into());
    let child = direct.target("S!D2").unwrap();
    assert_eq!(direct.cell(child).unwrap(), "-2j".into());
    let artifact::RestoredCheckpoint::Source { status, .. } = Workbook::load_snapshot()
        .prepared_policy(artifact::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap()
        .into_checkpoint()
        .unwrap()
    else {
        panic!("source-only reused cache")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedByRequest);
}
#[test]
fn xlsx_roundtrip_executes_complex_profiles_and_spills() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (cell, formula) in [
        ("A1", "IMABS({\"3+4i\";\"5+12i\"})"),
        ("D1", "IMCONJUGATE({\"i\";\"2j\"})"),
        ("G1", "IMCOSH(0)"),
        ("J1", "IMSUM({\"i\";\"2i\"})"),
    ] {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    let bytes = poirot::io::write_xlsx_to_buffer(&book).unwrap();
    let reopened = poirot::io::read_workbook_bytes(&bytes).unwrap();
    let mut evaluation = poirot::eval::overlay::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in [
        ("S!A2", 13.0.into()),
        ("S!D2", "-2j".into()),
        ("S!G1", "1".into()),
        ("S!J1", "3i".into()),
    ] {
        let target = evaluation.target(cell).unwrap();
        assert_eq!(evaluation.cell(target).unwrap(), expected);
    }
}
