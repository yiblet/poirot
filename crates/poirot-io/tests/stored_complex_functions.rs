//! Independent MS-XLSX function-table spelling checks for complex operations.
//! https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/5d1b6d44-6fc1-4ecd-8fef-0b27406cc2bf
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;
#[test]
fn complex_storage_prefixes_distinguish_legacy_and_newer_names() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let names = [
        "IMABS",
        "IMARGUMENT",
        "IMCONJUGATE",
        "IMCOS",
        "IMCOSH",
        "IMCOT",
        "IMCSC",
        "IMCSCH",
        "IMDIV",
        "IMEXP",
        "IMLN",
        "IMLOG10",
        "IMLOG2",
        "IMPOWER",
        "IMPRODUCT",
        "IMSEC",
        "IMSECH",
        "IMSIN",
        "IMSINH",
        "IMSQRT",
        "IMSUB",
        "IMSUM",
        "IMTAN",
    ];
    for (row, name) in names.iter().enumerate() {
        let formula = if ["IMDIV", "IMPOWER", "IMSUB"].contains(name) {
            format!("{name}(1,2)")
        } else {
            format!("{name}(1)")
        };
        sheet
            .set_formula(
                poirot_model::CellAddress {
                    row: row as u32,
                    col: 0,
                },
                formula,
            )
            .unwrap();
    }
    let bytes = write_xlsx_to_buffer(&book).unwrap();
    let reopened = read_workbook_bytes(&bytes).unwrap();
    for (row, name) in names.iter().enumerate() {
        let prefix = if [
            "IMCOSH", "IMCOT", "IMCSC", "IMCSCH", "IMSEC", "IMSECH", "IMSINH", "IMTAN",
        ]
        .contains(name)
        {
            "_xlfn."
        } else {
            ""
        };
        let args = if ["IMDIV", "IMPOWER", "IMSUB"].contains(name) {
            "1,2"
        } else {
            "1"
        };
        assert_eq!(
            reopened
                .sheet(0)
                .unwrap()
                .get(poirot_model::CellAddress {
                    row: row as u32,
                    col: 0
                })
                .unwrap()
                .formula
                .as_deref(),
            Some(format!("{prefix}{name}({args})").as_str()),
            "{name}"
        );
    }
}
