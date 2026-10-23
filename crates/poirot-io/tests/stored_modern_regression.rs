//! Legacy regression names preserve authored spelling through actual XLSX I/O.
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;
#[test]
fn modern_regression_spellings_roundtrip() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    let cases = [
        ("A1", "LINEST({1;2;2},{0;1;2},TRUE,TRUE)"),
        ("D1", "LOGEST({3;6;12},{0;1;2})"),
        ("G1", "GROWTH({3;6;12},{0;1;2},{3;4})"),
    ];
    for (c, f) in cases {
        s.set_formula(c.parse().unwrap(), f).unwrap();
    }
    let bytes = write_xlsx_to_buffer(&b).unwrap();
    let reopened = read_workbook_bytes(&bytes).unwrap();
    for (c, f) in cases {
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(c.parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some(f)
        );
    }
}
