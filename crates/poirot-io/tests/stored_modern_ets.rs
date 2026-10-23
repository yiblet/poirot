//! Authored ETS names and omitted optional arguments survive actual XLSX I/O.
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;
#[test]
fn modern_ets_spellings_roundtrip() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    let cases = [
        ("A1", "FORECAST.ETS(4,{1;2;3},{1;2;3},0)"),
        ("B1", "FORECAST.ETS.CONFINT(4,{1;2;3},{1;2;3},,0)"),
        ("C1", "FORECAST.ETS.SEASONALITY({1;2;3},{1;2;3})"),
        ("D1", "FORECAST.ETS.STAT({1;2;3},{1;2;3},8)"),
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
            Some(format!("_xlfn.{f}").as_str())
        );
    }
}
