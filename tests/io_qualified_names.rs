use poirot::model::{CellAddress, CellValue, Workbook};
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
#[test]
fn xlsx_retains_qualified_formula_and_scoped_name_identity() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Caller").unwrap();
    workbook.add_sheet("O'Brien").unwrap();
    workbook.define_name("Rate", "10", None).unwrap();
    workbook.define_name("Rate", "30", Some(1)).unwrap();
    let at = CellAddress { row: 0, col: 0 };
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(at, "'O''Brien'!Rate", CellValue::Number(-999.))
        .unwrap();
    let bytes = write_xlsx_to_buffer(&workbook).unwrap();
    let imported = read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    assert_eq!(imported.named_ranges, workbook.named_ranges);
    assert_eq!(
        imported
            .sheet(0)
            .unwrap()
            .get(at)
            .unwrap()
            .unwrap()
            .formula
            .as_deref(),
        Some("'O''Brien'!Rate")
    );
    assert_eq!(
        poirot_parsing::formula::parse_formula(
            imported
                .sheet(0)
                .unwrap()
                .get(at)
                .unwrap()
                .unwrap()
                .formula
                .as_ref()
                .unwrap()
        )
        .unwrap(),
        poirot_parsing::formula::Expr::QualifiedName {
            sheet: "O'Brien".into(),
            name: "Rate".into()
        }
    );
}
