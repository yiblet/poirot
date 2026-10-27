use poirot::model::{CellValue, Workbook};
#[test]
fn xlsx_round_trip_preserves_original_sheet_span_formula_text() {
    let mut book = Workbook::new();
    for name in ["O'Brien", "Middle", "Last Data"] {
        book.add_sheet(name).unwrap();
    }
    let formula = "SUM('O''Brien:Last Data'!$A1:B$2)";
    book.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value("C1", formula, CellValue::Number(-999.))
        .unwrap();
    let bytes = poirot_io::write_xlsx_to_buffer(&book).unwrap();
    let read = poirot_io::read_workbook_bytes::<poirot::model::Workbook>(&bytes).unwrap();
    let cell = read.sheet(0).unwrap().get("C1").unwrap();
    assert_eq!(cell.unwrap().formula.as_deref(), Some(formula));
}
