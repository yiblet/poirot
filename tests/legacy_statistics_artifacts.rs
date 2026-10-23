//! Legacy spelling preservation and evaluation through XLSX and saved artifacts.
use poirot::{CellValue, EvaluationContext, Overlay, Workbook, artifact};
fn near(v: CellValue, e: f64) {
    let CellValue::Number(a) = v else {
        panic!("{v:?}")
    };
    assert!((a - e).abs() < e.abs().max(1.) * 3e-11, "{a} != {e}");
}
fn cases() -> [(&'static str, f64); 23] {
    // Uniform beta, binomial counting, exponential/gamma shape 1, F(2,2)
    // survival 1/(1+x), Cauchy two-tail median, and equal-sample tests.
    [
        ("BETADIST(3,1,1,2,6)", 0.25),
        ("BETAINV(0.25,1,1,2,6)", 3.),
        ("BINOMDIST(1,4,0.5,FALSE)", 0.25),
        ("CHIDIST(2*LN(2),2)", 0.5),
        ("CHIINV(0.5,2)", 2. * 2f64.ln()),
        ("CHITEST({2,2;2,2},{2,2;2,2})", 1.),
        ("CONFIDENCE(0.05,1,1)", 1.9599639845400542),
        ("CRITBINOM(4,0.5,5/16)", 1.),
        ("EXPONDIST(2,0.5,TRUE)", 1. - (-1f64).exp()),
        ("FDIST(3,2,2)", 0.25),
        ("FINV(0.25,2,2)", 3.),
        ("FTEST({0;1;2},{2;3;4})", 1.),
        ("GAMMADIST(2,1,2,TRUE)", 1. - (-1f64).exp()),
        ("GAMMAINV(0.5,1,2)", 2. * 2f64.ln()),
        ("HYPGEOMDIST(1,2,2,4)", 2. / 3.),
        ("LOGINV(0.5,0,1)", 1.),
        ("LOGNORMDIST(1,0,1)", 0.5),
        ("NEGBINOMDIST(1,2,0.5)", 0.25),
        ("QUARTILE({0;4},1)", 1.),
        ("TINV(0.5,1)", 1.),
        ("TTEST({0;1;2},{0;1;2},2,2)", 1.),
        ("WEIBULL(2,1,2,TRUE)", 1. - (-1f64).exp()),
        ("ZTEST({0;1;2},1)", 0.5),
    ]
}

fn book() -> Workbook {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    for (i, (f, _)) in cases().into_iter().enumerate() {
        s.set_formula(format!("A{}", i + 1).parse().unwrap(), f)
            .unwrap();
    }
    b
}
#[test]
fn all_legacy_names_survive_actual_xlsx_spelling_and_execution_roundtrip() {
    let b = book();
    let bytes = poirot::io::write_xlsx_to_buffer(&b).unwrap();
    let reopened = poirot::io::read_workbook_bytes(&bytes).unwrap();
    for (i, (formula, _)) in cases().into_iter().enumerate() {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(format!("A{}", i + 1).parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(formula)
        );
    }
    let mut direct = poirot::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (i, (_, e)) in cases().into_iter().enumerate() {
        let t = direct.target(&format!("S!A{}", i + 1)).unwrap();
        near(direct.cell(t).unwrap(), e);
    }
}
#[test]
fn legacy_prepared_and_cold_artifacts_recover_all_contracts() {
    let mut overlay = Overlay::with_workbook(book()).build().unwrap();
    for (i, (_, e)) in cases().into_iter().enumerate() {
        let t = overlay.target(&format!("S!A{}", i + 1)).unwrap();
        near(
            overlay
                .evaluation(&EvaluationContext::default())
                .unwrap()
                .cell(t)
                .unwrap(),
            e,
        );
    }
    let bytes = overlay.save_snapshot().to_buffer().unwrap();
    let restored = Workbook::load_snapshot().from_bytes(&bytes).unwrap();
    assert_eq!(restored.prepared_status, artifact::PreparedStatus::Restored);
    let artifact::RestoredCheckpoint::Prepared(mut prepared) = restored.into_checkpoint().unwrap()
    else {
        panic!()
    };
    for (i, (_, e)) in cases().into_iter().enumerate() {
        let t = prepared.target(&format!("S!A{}", i + 1)).unwrap();
        near(
            prepared
                .evaluation(&EvaluationContext::default())
                .unwrap()
                .cell(t)
                .unwrap(),
            e,
        );
    }
    let restored = Workbook::load_snapshot()
        .prepared_policy(artifact::PreparedPolicy::SourceOnly)
        .from_bytes(&bytes)
        .unwrap();
    assert_eq!(
        restored.prepared_status,
        artifact::PreparedStatus::DiscardedByRequest
    );
    let artifact::RestoredCheckpoint::Source { workbook, .. } = restored.into_checkpoint().unwrap()
    else {
        panic!()
    };
    let mut direct = poirot::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    for (i, (_, e)) in cases().into_iter().enumerate() {
        let t = direct.target(&format!("S!A{}", i + 1)).unwrap();
        near(direct.cell(t).unwrap(), e);
    }
    let mut old = bytes;
    let previous = poirot::eval::overlay::snapshot::COMPATIBILITY_REVISION - 1;
    old[16..24].copy_from_slice(&previous.to_le_bytes());
    assert!(
        Workbook::load_snapshot()
            .prepared_policy(artifact::PreparedPolicy::RequireCompatible)
            .from_bytes(&old)
            .is_err()
    );
    let restored = Workbook::load_snapshot().from_bytes(&old).unwrap();
    assert_eq!(
        restored.prepared_status,
        artifact::PreparedStatus::DiscardedIncompatible
    );
    let artifact::RestoredCheckpoint::Source { workbook, .. } = restored.into_checkpoint().unwrap()
    else {
        panic!()
    };
    let mut direct = poirot::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    let t = direct.target("S!A1").unwrap();
    near(direct.cell(t).unwrap(), 0.25);
}
