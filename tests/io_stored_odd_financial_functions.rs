//! Legacy irregular financial function spellings roundtrip without future-function prefixes.
use poirot::model::{CellAddress, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
#[test]
fn odd_financial_function_storage_names_roundtrip() {
    let mut b = Workbook::new();
    let sheet = b.add_sheet("S").unwrap();
    let names = [
        "AMORDEGRC",
        "AMORLINC",
        "ODDFPRICE",
        "ODDFYIELD",
        "ODDLPRICE",
        "ODDLYIELD",
    ];
    for (row, name) in names.iter().enumerate() {
        sheet
            .set_formula(
                CellAddress {
                    row: row as u32,
                    col: 0,
                },
                format!("{name}(1)"),
            )
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&b).unwrap();
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (row, name) in names.iter().enumerate() {
        let prefix = "";
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(CellAddress {
                    row: row as u32,
                    col: 0
                })
                .unwrap()
                .formula
                .as_deref(),
            Some(format!("{prefix}{name}(1)").as_str())
        );
    }
}
