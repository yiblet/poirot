//! Every admitted spelling survives XLSX with the existing per-name prefix policy.
use poirot::model::{CellAddress, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
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
fn all_modern_sample_spellings_roundtrip() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (i, (formula, _)) in cases().iter().enumerate() {
        sheet
            .set_formula(
                CellAddress {
                    row: i as u32,
                    col: 0,
                },
                *formula,
            )
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (i, (formula, _)) in cases().iter().enumerate() {
        let future = [
            "CHISQ.TEST",
            "F.TEST",
            "MODE.MULT",
            "MODE.SNGL",
            "PERCENTILE.EXC",
            "PERMUTATIONA",
            "QUARTILE.EXC",
            "RANK.AVG",
            "SKEW.P",
            "T.TEST",
            "Z.TEST",
        ]
        .iter()
        .any(|name| formula.starts_with(&format!("{name}(")));
        let expected = if future {
            format!("_xlfn.{formula}")
        } else {
            formula.to_string()
        };
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(CellAddress {
                    row: i as u32,
                    col: 0
                })
                .unwrap()
                .unwrap()
                .formula
                .as_deref(),
            Some(expected.as_str())
        );
    }
}
