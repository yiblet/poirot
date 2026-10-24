//! Public saved-source recovery and XLSX modern_inverse execution contracts.
use poirot::{CellValue, EvaluationContext, Workbook, artifact, eval::overlay::Overlay};
#[test]
fn incompatible_modern_inverse_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1".parse().unwrap(), 0.1).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "BETA.INV(A1,1,1)")
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
        0.2.into()
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
        0.20.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 0.2.into());
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
        0.20.into()
    );
}
#[test]
fn stored_modern_inverse_names_execute_after_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let cases = [
        ("A1", "BETA.INV(0.25,2,1)", 0.5),
        ("A2", "BINOM.INV(4,0.5,5/16)", 1.),
        ("A3", "CHISQ.INV(0.5,2)", 2. * 2f64.ln()),
        ("A4", "CHISQ.INV.RT(0.25,2)", 2. * 4f64.ln()),
        ("A5", "CONFIDENCE.NORM(0.05,1,1)", 1.9599639845400542),
        ("A6", "CONFIDENCE.T(0.5,2,2)", 2f64.sqrt()),
        ("A7", "F.INV(0.25,2,2)", 1. / 3.),
        ("A8", "F.INV.RT(0.25,2,2)", 3.),
        ("A9", "GAMMA.INV(0.5,1,2)", 2. * 2f64.ln()),
        ("A10", "LOGNORM.INV(0.5,0,1)", 1.),
        ("A11", "T.INV(0.75,2)", (2f64 / 3.).sqrt()),
        ("A12", "T.INV.2T(0.5,1)", 1.),
    ];
    for (cell, formula, _) in cases {
        sheet.set_formula(cell.parse().unwrap(), formula).unwrap();
    }
    sheet
        .set_formula("D1".parse().unwrap(), "BETA.INV({0.25;0.5},1,1)")
        .unwrap();
    sheet
        .set_formula("G1".parse().unwrap(), "BINOM.INV(4,0.5,{0.25;0.5})")
        .unwrap();
    let bytes = poirot::io::write_xlsx_to_buffer(&book).unwrap();
    let book = poirot::io::read_workbook_bytes(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(book)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, expected) in cases.into_iter().map(|(cell, _, e)| (cell, e)).chain([
        ("D1", 0.25),
        ("D2", 0.5),
        ("G2", 2.),
    ]) {
        let target = direct.target(&format!("S!{cell}")).unwrap();
        let CellValue::Number(actual) = direct.cell(target).unwrap() else {
            panic!()
        };
        assert!(
            (actual - expected).abs() <= expected.abs() * 2e-12,
            "{cell}: {actual} != {expected}"
        );
    }
}
