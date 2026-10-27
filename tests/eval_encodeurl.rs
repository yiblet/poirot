//! Independently derived ENCODEURL contracts from Microsoft's documented example
//! and RFC 3986 sections 2.1, 2.3 and 2.5 (no upstream code or fixtures copied).
//! https://support.microsoft.com/en-us/excel/functions/encodeurl-function
//! https://www.rfc-editor.org/rfc/rfc3986.html#section-2
use poirot::eval::{
    EvaluationBudgets, EvaluationOutcome, RunConfiguration, run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellError, CellValue, ExcelText, Workbook};

fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    book
}

fn evaluate(book: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        CellAddress { row: 0, col: 10 },
        formula,
        &RunConfiguration::default(),
        Default::default(),
    )
}

fn value(formula: &str) -> CellValue {
    match evaluate(&book(), formula) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}

#[test]
fn encodes_documented_url_reserved_characters_and_utf8() {
    // Microsoft's example establishes escaping of scheme/path delimiters and
    // spaces while preserving dots. Other expectations follow UTF-8 octets
    // and RFC 3986's ASCII unreserved set, rather than a live Excel parity run.
    for (formula, expected) in [
        (
            "ENCODEURL(\"http://contoso.sharepoint.com/Finance/Profit and Loss Statement.xlsx\")",
            "http%3A%2F%2Fcontoso.sharepoint.com%2FFinance%2FProfit%20and%20Loss%20Statement.xlsx",
        ),
        ("ENCODEURL(\"AZaz09-._~\")", "AZaz09-._~"),
        (
            "ENCODEURL(\" :/?#[]@!$&'()*+,;=%\")",
            "%20%3A%2F%3F%23%5B%5D%40%21%24%26%27%28%29%2A%2B%2C%3B%3D%25",
        ),
        (
            "ENCODEURL(\"café 中😀\")",
            "caf%C3%A9%20%E4%B8%AD%F0%9F%98%80",
        ),
        ("ENCODEURL(\"\")", ""),
        ("ENCODEURL(42)", "42"),
        ("ENCODEURL(TRUE)", "TRUE"),
        ("ENCODEURL(A1)", ""),
        ("_xlfn.ENCODEURL(\"x y\")", "x%20y"),
        ("INDEX(ENCODEURL({\"a b\",\"c/d\"}),1,2)", "c%2Fd"),
    ] {
        assert_eq!(
            value(formula),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
}

#[test]
fn propagates_errors_checks_arity_and_enforces_output_length() {
    for (formula, expected) in [
        ("ENCODEURL()", CellError::Value),
        ("ENCODEURL(\"a\",\"b\")", CellError::Value),
        ("ENCODEURL(#N/A)", CellError::NA),
        ("ENCODEURL(1/0)", CellError::Div0),
        ("ENCODEURL(REPT(\" \",10923))", CellError::Value),
    ] {
        assert_eq!(value(formula), CellValue::Error(expected), "{formula}");
    }
    assert_eq!(
        value("LEN(ENCODEURL(REPT(\"a\",32767)))"),
        CellValue::Number(32767.0)
    );
    assert_eq!(
        value("LEN(ENCODEURL(REPT(\" \",10922)&\"a\"))"),
        CellValue::Number(32767.0)
    );
}

#[test]
fn refuses_unpaired_utf16_without_lossy_replacement() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_value(
            CellAddress { row: 0, col: 0 },
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    assert!(matches!(
        evaluate(&book, "IFERROR(ENCODEURL(A1),99)"),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn output_budget_refusal_is_not_catchable() {
    let book = book();
    let result = run_formula_with_configuration(
        &book,
        0,
        CellAddress { row: 0, col: 10 },
        "IFERROR(ENCODEURL(\"? ? ? ?\"),99)",
        &RunConfiguration::default(),
        EvaluationBudgets {
            text_bytes: 16,
            ..Default::default()
        },
    );
    assert!(
        matches!(result, EvaluationOutcome::Aborted { .. }),
        "{result:?}"
    );
}
