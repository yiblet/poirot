use poirot_model::{CellAddress, CellValue, Workbook};
#[test]
fn qualified_names_fail_explicitly_instead_of_erasing_scope() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.add_sheet("Other").unwrap();
    workbook.define_name("Rate", "20", Some(0)).unwrap();
    workbook.define_name("Rate", "30", Some(1)).unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            CellAddress { row: 0, col: 0 },
            "Other!Rate",
            CellValue::Number(-999.),
        )
        .unwrap();
    let error = poirot_xls::write_xls_to_buffer(&workbook)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("sheet-qualified defined name is unsupported"),
        "{error}"
    );
    assert!(error.contains("Other!Rate"), "{error}");
}
