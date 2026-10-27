use poirot::model::{CellAddress, CellError, CellValue, Workbook};
fn value(workbook: &Workbook, sheet: usize, source: &str) -> CellValue {
    match poirot::eval::run_formula(
        workbook,
        sheet,
        CellAddress { row: 9, col: 7 },
        source,
        Default::default(),
        Default::default(),
    ) {
        poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
#[test]
fn computed_ranges_normalize_offset_geometry_through_full_formula_parsing() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    // Retained negative-size policy normalizes B3 to A2:B3; native Excel unverified.
    for source in [
        "ROWS(OFFSET(B3,0,0,-2,-2):C4)",
        "COLUMNS(OFFSET(B3,0,0,-2,-2):C4)",
    ] {
        assert_eq!(value(&w, 0, source), CellValue::Number(3.), "{source}");
    }
    assert_eq!(value(&w, 0, "ROWS((A1:B3) (B2:C4))"), CellValue::Number(2.));
    assert_eq!(
        value(&w, 0, "ROWS(IF(TRUE,A1,B1):C4)"),
        CellValue::Number(4.)
    );
    assert_eq!(
        value(&w, 0, "ROWS(ABS(1):C4)"),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn names_remain_lazy_scoped_operands_instead_of_speculative_columns() {
    let mut w = Workbook::new();
    w.add_sheet("Data").unwrap();
    w.add_sheet("Other").unwrap();
    w.define_name("Start", "A1", None).unwrap();
    w.define_name("End", "B3", None).unwrap();
    w.define_name("Z", "C4", None).unwrap();
    w.define_name("Z", "C2", Some(0)).unwrap();
    assert_eq!(value(&w, 0, "ROWS(Start:End)"), CellValue::Number(3.));
    assert_eq!(value(&w, 0, "ROWS(A1:Z)"), CellValue::Number(2.));
    assert_eq!(value(&w, 1, "ROWS(A1:Z)"), CellValue::Number(4.));
    // Parentheses distinguish a range RHS from the lexical A1:Data sheet span.
    // Keep both endpoints on Data: cross-sheet RangeEval parity is a separate gap.
    assert_eq!(value(&w, 0, "ROWS(A1:('Data'!Z))"), CellValue::Number(2.));
    w.named_ranges
        .iter_mut()
        .find(|name| name.name == "Z" && name.sheet_scope == Some(0))
        .unwrap()
        .formula = "C5".into();
    assert_eq!(value(&w, 0, "ROWS(A1:Z)"), CellValue::Number(5.));
    assert_eq!(
        value(&w, 0, "ROWS(A1:MissingName)"),
        CellValue::Error(CellError::Name)
    );
    w.define_name("Loop", "Loop", None).unwrap();
    assert_eq!(
        value(&w, 0, "ROWS(A1:Loop)"),
        CellValue::Error(CellError::Ref)
    );
    // Range operands must address one sheet; multi-sheet ranges are unsupported.
    assert_eq!(
        value(&w, 0, "ROWS(Data!A1:Other!C4)"),
        CellValue::Error(CellError::Ref)
    );
}
