use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_value("A1".parse().unwrap(), "mcmxii").unwrap();
    sheet.set_value("A2".parse().unwrap(), 999.).unwrap();
    sheet.set_value("A3".parse().unwrap(), false).unwrap();
    sheet
        .set_value("A4".parse().unwrap(), CellValue::Error(CellError::NA))
        .unwrap();
    book
}
fn config() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}
fn run(
    book: &Workbook,
    formula: &str,
    config: &RunConfiguration,
    budget: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        book,
        0,
        CellAddress { row: 0, col: 25 },
        formula,
        config,
        budget,
    )
}
fn value(book: &Workbook, formula: &str, config: &RunConfiguration) -> CellValue {
    match run(book, formula, config, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
#[test]
fn roman_defaults_and_arabic_parsing_follow_explicit_domains() {
    let book = book();
    let config = config();
    for (formula, expected) in [
        ("ROMAN(999)", "CMXCIX"),
        ("ROMAN(999,)", "CMXCIX"),
        ("ROMAN(A2)", "CMXCIX"),
        ("ROMAN(499,A3)", "ID"),
        ("ROMAN(499,A5)", "CDXCIX"),
        ("ROMAN(499,TRUE)", "CDXCIX"),
        ("ROMAN(3999.9)", "MMMCMXCIX"),
        ("ROMAN(499,4.9)", "ID"),
    ] {
        assert_eq!(
            value(&book, formula, &config),
            CellValue::Text(expected.into()),
            "{formula}"
        );
    }
    for (formula, expected) in [
        ("ARABIC(A1)", 1912.),
        ("ARABIC(A5)", 0.),
        ("ARABIC({\"mcmxii\"})", 1912.),
        ("ARABIC(\"-MMXI\")", -2011.),
        ("ARABIC(\" X \")", 10.),
        ("ARABIC(\"\")", 0.),
        ("ARABIC(\"MXMVII\")", 1997.),
        ("ARABIC(\"IIII\")", 4.),
    ] {
        assert_eq!(
            value(&book, formula, &config),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    for (formula, error) in [
        ("ROMAN(-0.9)", CellError::Value),
        ("ROMAN(4000)", CellError::Value),
        ("ROMAN(499,-0.9)", CellError::Value),
        ("ROMAN(499,5)", CellError::Value),
        ("ARABIC(A4)", CellError::NA),
        ("ARABIC(A2)", CellError::Value),
        ("ARABIC(A3)", CellError::Value),
        ("ARABIC(\"+IV\")", CellError::Value),
        ("ARABIC(\"I I\")", CellError::Value),
        ("ARABIC(1/0)", CellError::Div0),
        ("ARABIC(UNKNOWN_FUNCTION(),1)", CellError::Value),
        ("ARABIC()", CellError::Value),
    ] {
        assert_eq!(
            value(&book, formula, &config),
            CellValue::Error(error),
            "{formula}"
        );
    }
}
#[test]
fn roman_all_forms_keep_formatter_decoder_inverse() {
    // Inverse invariant, not an independently observed Excel expectation for
    // every spelling produced by the existing formatter.
    for number in 0..=3999u16 {
        for form in 0..=4 {
            let text = poirot_functions_text::roman(number, form);
            assert_eq!(
                poirot_coercionv2::roman::coercev2_arabic_text(&text),
                Ok(i32::from(number)),
                "{number}, {form}: {text}"
            );
        }
    }
}
#[test]
fn roman_contextual_numeric_text_and_arabic_operational_failures() {
    let book = book();
    let locale = config().with_number_locale(
        poirot_parsing::number::NumberLocaleV2::new(",", Some("."), Some("€"), "%", ":").unwrap(),
    );
    assert_eq!(
        value(&book, r#"ROMAN("€499,9","4,9")"#, &locale),
        CellValue::Text("ID".into())
    );
    for (formula, budget) in [
        (
            "IFERROR(ARABIC(A1),0)",
            EvaluationBudgets {
                text_bytes: 3,
                ..Default::default()
            },
        ),
        (
            "IFERROR(ARABIC(A1),0)",
            EvaluationBudgets {
                run_work: 3,
                ..Default::default()
            },
        ),
        (
            "IFERROR(ROMAN(999),0)",
            EvaluationBudgets {
                run_work: 10,
                ..Default::default()
            },
        ),
        (
            "IFERROR(ROMAN(999),0)",
            EvaluationBudgets {
                text_bytes: 3,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(
                run(&book, formula, &config(), budget),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    assert_eq!(
        value(&book, "IFERROR(ARABIC(1),42)", &config()),
        CellValue::Number(42.)
    );
}
