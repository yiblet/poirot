//! Evaluator revision rejection retains authored utility formulas and edits.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
#[test]
fn old_preparation_recovers_utility_source_and_recalculates() {
    let mut book = Workbook::new();
    let s = book.add_sheet("S").unwrap();
    s.set_value("J1", 2.).unwrap();
    s.set_formula("A1", "MUNIT(J1)").unwrap();
    s.set_formula("G1", "MULTINOMIAL(J1,3)").unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!J1").unwrap();
    let output = overlay.target("S!G1").unwrap();
    overlay.set_value(input, 3.).unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(output).unwrap(),
        20.0.into()
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
        panic!("old evaluator state accepted")
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
        CellValue::Number(3.)
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(context)
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 20.0.into());
    let child = direct.target("S!C3").unwrap();
    assert_eq!(direct.cell(child).unwrap(), 1.0.into());
    let artifact::RestoredCheckpoint::Source { status, .. } =
        poirot::eval::artifact::SnapshotLoadBuilder::default()
            .prepared_policy(artifact::PreparedPolicy::SourceOnly)
            .from_bytes(&bytes)
            .unwrap()
            .into_checkpoint()
            .unwrap()
    else {
        panic!("source-only reused preparation")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedByRequest);
}
#[test]
fn utility_xlsx_roundtrip_executes_prefixed_functions_and_spills() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    s.set_formula("A1", "MUNIT(2)").unwrap();
    s.set_formula("D1", "NETWORKDAYS.INTL(45292,45298)")
        .unwrap();
    s.set_formula("G1", "ACOT(1)").unwrap();
    let bytes = b.save_xlsx().to_buffer().unwrap();
    let reopened = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut e = poirot::eval::overlay::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (cell, n) in [
        ("S!B2", 1.),
        ("S!D1", 5.),
        ("S!G1", std::f64::consts::FRAC_PI_4),
    ] {
        let target = e.target(cell).unwrap();
        assert_eq!(e.cell(target).unwrap(), n.into());
    }
}
