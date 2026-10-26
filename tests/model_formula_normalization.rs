//! Public normalization contracts for owned formula inputs and cell replacement.
use poirot::model::{Cell, CellValue, Workbook};

#[test]
fn owned_formula_sources_keep_normalization_and_empty_rejection() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    let address = "A1".parse().unwrap();
    // The documented model contract removes whitespace and one leading equals.
    for (input, constructed, stored) in [
        ("A1+$B$2", "A1+$B$2", "A1+$B$2"),
        ("  = A1+$B$2 \n", "A1+$B$2", "A1+$B$2"),
        (
            "\u{2003}=\u{2003}SUM(A1:A2)\u{2003}",
            "SUM(A1:A2)",
            "SUM(A1:A2)",
        ),
        ("==1", "=1", "1"),
    ] {
        let cell = Cell::with_formula(input.to_owned(), CellValue::Blank).unwrap();
        assert_eq!(cell.formula.as_deref(), Some(constructed));
        sheet.set(address, cell).unwrap();
        assert_eq!(sheet.get(address).unwrap().formula.as_deref(), Some(stored));
    }
    let previous = sheet.get(address).unwrap().clone();
    for input in ["", "  ", "=", " \u{2003}= \n"] {
        assert!(Cell::with_formula(input.to_owned(), CellValue::Blank).is_err());
        let mut invalid = Cell::new(1.);
        invalid.formula = Some(input.to_owned());
        assert!(sheet.set(address, invalid).is_err());
        assert_eq!(sheet.get(address), Some(&previous));
    }
}
