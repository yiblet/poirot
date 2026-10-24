//! Actual XLSX execution and prepared-state recovery of modern regression arrays.
use poirot::{CellValue, EvaluationContext, Workbook, artifact, eval::overlay::Overlay};
fn near(v: CellValue, e: f64) {
    let CellValue::Number(a) = v else {
        panic!("{v:?}")
    };
    assert!((a - e).abs() < e.abs().max(1.) * 1e-10, "{a} != {e}");
}
#[test]
fn regression_arrays_execute_after_actual_xlsx_roundtrip() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    for (c, f) in [
        ("A1", "LINEST({1;2;2},{0;1;2},TRUE,TRUE)"),
        ("D1", "LOGEST({3;6;12},{0;1;2})"),
        ("G1", "GROWTH({3;6;12},{0;1;2},{3;4})"),
    ] {
        s.set_formula(c.parse().unwrap(), f).unwrap();
    }
    let bytes = poirot::io::write_xlsx_to_buffer(&b).unwrap();
    let reopened = poirot::io::read_workbook_bytes(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (c, e) in [
        ("A1", 0.5),
        ("B1", 7. / 6.),
        ("A3", 0.75),
        ("B5", 1. / 6.),
        ("D1", 2.),
        ("E1", 3.),
        ("G2", 48.),
    ] {
        let t = direct.target(&format!("S!{c}")).unwrap();
        near(direct.cell(t).unwrap(), e);
    }
}
#[test]
fn incompatible_regression_preparation_recovers_source_and_overlay_edits() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    s.set_value("A1".parse().unwrap(), 3.).unwrap();
    s.set_formula("D1".parse().unwrap(), "GROWTH({3;6;12},{0;1;2},A1)")
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
        48.,
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
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    near(direct.cell(output).unwrap(), 48.);
}
