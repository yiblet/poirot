//! Financial spellings preserve normative modern future-function prefixes.
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::{CellAddress, Workbook};
#[test]
fn financial_function_storage_names_roundtrip() {
    let mut b = Workbook::new();
    let sheet = b.add_sheet("S").unwrap();
    let names = [
        "CUMIPMT",
        "CUMPRINC",
        "DB",
        "DDB",
        "EFFECT",
        "FVSCHEDULE",
        "ISPMT",
        "NOMINAL",
        "PDURATION",
        "RRI",
        "SLN",
        "SYD",
        "VDB",
        "XIRR",
        "XNPV",
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
    let reopened = read_workbook_bytes(&bytes).unwrap();
    for (row, name) in names.iter().enumerate() {
        let prefix = if matches!(*name, "PDURATION" | "RRI") {
            "_xlfn."
        } else {
            ""
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
                .formula
                .as_deref(),
            Some(format!("{prefix}{name}(1)").as_str())
        );
    }
}
