//! MS-XLSX storage spelling separates newer bit functions from legacy engineering.
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
use poirot::model::{CellAddress, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
#[test]
fn engineering_storage_prefixes_and_legacy_spellings() {
    let mut b = Workbook::new();
    let s = b.add_sheet("S").unwrap();
    let names = [
        "BIN2HEX",
        "BIN2OCT",
        "BITAND",
        "BITLSHIFT",
        "BITOR",
        "BITRSHIFT",
        "BITXOR",
        "CONVERT",
        "DEC2OCT",
        "HEX2BIN",
        "HEX2OCT",
        "OCT2BIN",
        "OCT2HEX",
    ];
    for (row, name) in names.iter().enumerate() {
        s.set_formula(
            CellAddress {
                row: row as u32,
                col: 0,
            },
            if *name == "CONVERT" {
                "CONVERT(1,\"m\",\"m\")".into()
            } else {
                format!("{name}(1,2)")
            },
        )
        .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&b).unwrap();
    let reopened = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    for (row, name) in names.iter().enumerate() {
        let expected = if *name == "CONVERT" {
            "CONVERT(1,\"m\",\"m\")".into()
        } else {
            format!(
                "{}{name}(1,2)",
                if name.starts_with("BIT") {
                    "_xlfn."
                } else {
                    ""
                }
            )
        };
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(CellAddress {
                    row: row as u32,
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
