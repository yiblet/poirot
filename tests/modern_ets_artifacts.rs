//! ETS execution through actual XLSX and prepared-state recovery.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
fn near(v: CellValue, e: f64) {
    let CellValue::Number(a) = v else {
        panic!("{v:?}")
    };
    assert!((a - e).abs() < e.abs().max(1.0) * 1e-9, "{a} != {e}");
}
#[test]
fn ets_executes_after_actual_xlsx_roundtrip() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    for (c, f) in [
        ("A1", "FORECAST.ETS(4,{1;2;3},{1;2;3},0)"),
        ("B1", "FORECAST.ETS.CONFINT(4,{1;2;3},{1;2;3},,0)"),
        ("C1", "FORECAST.ETS.SEASONALITY({1;2;3},{1;2;3})"),
        ("D1", "FORECAST.ETS.STAT({1;2;3},{1;2;3},8)"),
    ] {
        s.set_formula(c, f).unwrap();
    }
    let bytes = b.save_xlsx().to_buffer().unwrap();
    let reopened = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (c, e) in [("A1", 4.0), ("B1", 0.0), ("C1", 0.0), ("D1", 1.0)] {
        let t = direct.target(&format!("S!{c}")).unwrap();
        near(direct.cell(t).unwrap(), e);
    }
}
#[test]
fn incompatible_ets_preparation_recovers_source_and_overlay_edits() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    s.set_value("A1", 3.).unwrap();
    s.set_formula("D1", "FORECAST.ETS(A1,{3;5;7},{1;2;3},0)")
        .unwrap();
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 4.).unwrap();
    near(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        9.,
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
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    near(direct.cell(output).unwrap(), 9.);
}
