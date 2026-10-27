//! Source recovery and actual XLSX execution of all 21 modern sample functions.
use poirot::calculation::EvaluationContext;
use poirot::eval::artifact;
use poirot::eval::overlay::Overlay;
use poirot::{CellValue, Workbook};
fn cases() -> [(&'static str, f64); 21] {
    [
        ("CHISQ.TEST({3,1;1,3},{2,2;2,2})", 0.15729920705028513),
        ("F.TEST({0;1;2},{0;2;4})", 0.4),
        ("FISHER(0.5)", 3f64.ln() * 0.5),
        ("FISHERINV(0)", 0.),
        ("HARMEAN(2,6)", 3.),
        ("KURT(-1,-1,1,1)", -6.),
        ("MODE.MULT({7;2;7;2})", 7.),
        ("MODE.SNGL({7;2;7;2})", 7.),
        ("PERCENTILE.EXC({0;10;20},3/8)", 5.),
        ("PERMUT(5,3)", 60.),
        ("PERMUTATIONA(2,4)", 16.),
        ("PROB({0;1;2},{0.25;0.25;0.5},1,2)", 0.75),
        ("QUARTILE.EXC({0;10;20},2)", 10.),
        ("RANK.AVG(2,{2;2;7})", 2.5),
        ("RSQ({1;0;1},{0;1;2})", 0.),
        ("SKEW(-1,0,1)", 0.),
        ("SKEW.P(-1,0,1)", 0.),
        ("STEYX({1;0;1},{0;1;2})", (2f64 / 3.).sqrt()),
        ("T.TEST({0;2},{2;3;4},2,3)", 0.2481885380840121),
        ("TRIMMEAN({-100;1;2;3;100},0.4)", 2.),
        ("Z.TEST({0;1;2},1)", 0.5),
    ]
}
#[test]
fn all_modern_sample_names_execute_after_actual_xlsx_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (i, (formula, _)) in cases().iter().enumerate() {
        sheet
            .set_formula(
                poirot::CellAddress {
                    row: 0,
                    col: i as u32 * 2,
                },
                *formula,
            )
            .unwrap();
    }
    let bytes = book.save_xlsx().to_buffer().unwrap();
    let reopened = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(reopened)
        .build(EvaluationContext::default())
        .unwrap();
    for (i, (_, expected)) in cases().iter().enumerate() {
        let target = poirot::calculation::EvaluationTarget {
            sheet: 0,
            address: poirot::CellAddress {
                row: 0,
                col: i as u32 * 2,
            },
        };
        let CellValue::Number(actual) = direct.cell(target).unwrap() else {
            panic!()
        };
        assert!((actual - expected).abs() <= expected.abs().max(1.) * 3e-12);
    }
    let target = direct.target("S!M2").unwrap();
    assert_eq!(direct.cell(target).unwrap(), 2f64.into());
}

#[test]
fn incompatible_modern_sample_preparation_preserves_formulas_and_overlay_edits() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    sheet.set_value("A1", 2.0).unwrap();
    sheet.set_formula("D1", "RANK.AVG(A1,{2;2;7})").unwrap();
    let mut overlay = Overlay::with_workbook(book).build().unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!D1").unwrap();
    overlay.set_value(input, 7.0).unwrap();
    assert_eq!(
        overlay
            .evaluation(&EvaluationContext::default())
            .unwrap()
            .cell(output)
            .unwrap(),
        1.0.into()
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
    assert_eq!(
        workbook
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .unwrap()
            .value,
        7.00.into()
    );
    let mut direct = poirot::eval::overlay::Evaluation::with_workbook(workbook)
        .build(EvaluationContext::default())
        .unwrap();
    assert_eq!(direct.cell(output).unwrap(), 1.0.into());
    let artifact::RestoredCheckpoint::Source { workbook, status } =
        poirot::eval::artifact::SnapshotLoadBuilder::default()
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
        workbook
            .sheet(0)
            .unwrap()
            .get(input.address)
            .unwrap()
            .unwrap()
            .value,
        7.00.into()
    );
}
