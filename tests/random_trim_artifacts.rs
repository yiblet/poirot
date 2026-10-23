//! Saved evaluator compatibility is independent of authored workbook source.
use poirot::{CellValue, EvaluationContext, Overlay, OverlayPolicy, Workbook, artifact};
#[test]
fn previous_random_trim_preparation_recovers_authored_source_and_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A3".parse().unwrap(), 8.0).unwrap();
    sheet
        .set_formula("D1".parse().unwrap(), "TRIMRANGE(A1:A5)")
        .unwrap();
    sheet
        .set_formula("G1".parse().unwrap(), "RANDARRAY(2,2)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A3").unwrap();
    let trimmed = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 11.0).unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay.evaluation(&context).unwrap().cell(trimmed).unwrap(),
        11.0.into()
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
    let restored = Workbook::load_snapshot().from_bytes(&bytes).unwrap();
    let artifact::RestoredCheckpoint::Source { workbook, status } =
        restored.into_checkpoint().unwrap()
    else {
        panic!("old preparation accepted")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedIncompatible);
    assert_eq!(
        workbook.sheet(0).unwrap().get(input.address).unwrap().value,
        CellValue::Number(11.0)
    );
    let mut direct = poirot::Evaluation::with_workbook(workbook)
        .with_policy(OverlayPolicy::default())
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(trimmed).unwrap(), 11.0.into());
    let random = direct.target("S!H2").unwrap();
    assert!(
        matches!(direct.cell(random).unwrap(), CellValue::Number(n) if (0.0..1.0).contains(&n))
    );
    let recovered = Workbook::load_snapshot()
        .prepared_policy(artifact::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap();
    let artifact::RestoredCheckpoint::Source { status, .. } = recovered.into_checkpoint().unwrap()
    else {
        panic!("source policy reused state")
    };
    assert_eq!(status, artifact::PreparedStatus::DiscardedByRequest);
}
