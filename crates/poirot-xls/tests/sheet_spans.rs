use poirot_model::{CellValue, Workbook};
use poirot_xls::{XlsError, write_xls_to_buffer};
fn unsupported(error: &XlsError) -> bool {
    match error {
        XlsError::Unsupported(detail) => detail.contains("3D sheet-span"),
        XlsError::Context { source, .. } => unsupported(source),
        _ => false,
    }
}
#[test]
fn unverified_biff_span_encoding_has_explicit_unsupported_disposition() {
    for formula in ["SUM(Sheet1:Sheet3!A1)", "SUM(Sheet1:Sheet3!A1:B2)"] {
        let mut book = Workbook::new();
        for name in ["Sheet1", "Sheet2", "Sheet3"] {
            book.add_sheet(name).unwrap();
        }
        book.sheet_mut(0)
            .unwrap()
            .set_formula_with_cached_value("C1".parse().unwrap(), formula, CellValue::Number(-999.))
            .unwrap();
        assert!(unsupported(&write_xls_to_buffer(&book).unwrap_err()));
    }
}
