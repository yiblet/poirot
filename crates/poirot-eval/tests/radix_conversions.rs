use poirot_eval::{EvaluationBudgets, EvaluationOutcome, run_formula};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet
        .set_value(CellAddress { row: 0, col: 0 }, "15")
        .unwrap();
    sheet
        .set_value(CellAddress { row: 1, col: 0 }, true)
        .unwrap();
    book
}
fn evaluate(book: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        book,
        0,
        CellAddress { row: 0, col: 3 },
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{formula}: {outcome:?}"),
    }
}
#[test]
fn radix_conversions_share_checked_digits_and_explicit_domains() {
    let book = book();
    for (formula, text) in [
        ("BASE(255,2,3)", "11111111"),
        ("BASE(15.9,16.9,3.9)", "00F"),
        ("BASE(0,10)", "0"),
        ("BASE(,16)", "0"),
        ("BASE(A1,16)", "F"),
        ("BASE(A2,16)", "1"),
        ("BASE({15},16)", "F"),
        ("BASE(A3,16)", "0"),
        ("BASE(15,16,)", "F"),
    ] {
        assert_eq!(
            evaluate(&book, formula),
            CellValue::Text(text.into()),
            "{formula}"
        );
    }
    for (formula, n) in [
        ("DECIMAL(\"zap\",36)", 45745.0),
        ("DECIMAL(111,2)", 7.0),
        ("DECIMAL(\"FF\",16)", 255.0),
        ("DECIMAL(A1,16)", 21.0),
        ("DECIMAL(A2,16)", 1.0),
        ("DECIMAL(A3,16)", 0.0),
        ("DECIMAL(,16)", 0.0),
        ("DECIMAL(\"\",16)", 0.0),
        ("DECIMAL({\"ff\"},16)", 255.0),
        ("DECIMAL(\" F\",16)", 15.0),
        ("DECIMAL(BASE(9007199254740991,36),36)", 9007199254740991.0),
    ] {
        assert_eq!(evaluate(&book, formula), CellValue::Number(n), "{formula}");
    }
    for formula in [
        "BASE(-0.5,2)",
        "BASE(9007199254740992,2)",
        "BASE(1,1.9)",
        "BASE(1,37)",
        "BASE(1,16,-0.5)",
        "BASE(1,16,256)",
        "BASE(1,16,1E99)",
        "DECIMAL(\"2\",2)",
        "DECIMAL(\"+F\",16)",
        "DECIMAL(\"F \",16)",
        "DECIMAL(\"-F\",16)",
        "DECIMAL(\"F\",37)",
    ] {
        assert_eq!(
            evaluate(&book, formula),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
    for formula in ["BASE(-1,1/0)", "BASE(1,1,1/0)", "DECIMAL(\"?\",1/0)"] {
        assert_eq!(evaluate(&book, formula), CellValue::Error(CellError::Div0));
    }
    for formula in [
        "BASE(\"\",2)",
        "BASE(UNKNOWN_FUNCTION())",
        "DECIMAL(UNKNOWN_FUNCTION())",
    ] {
        assert_eq!(evaluate(&book, formula), CellValue::Error(CellError::Value));
    }
}
#[test]
fn radix_operational_budgets_escape_iferror() {
    let book = book();
    for (formula, budget) in [
        (
            "IFERROR(BASE(1,2,255),0)",
            EvaluationBudgets {
                text_bytes: 254,
                ..Default::default()
            },
        ),
        (
            "IFERROR(BASE(1,2,255),0)",
            EvaluationBudgets {
                materialized_values: 254,
                ..Default::default()
            },
        ),
        (
            "IFERROR(BASE(1,2,255),0)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(DECIMAL(A1,16),0)",
            EvaluationBudgets {
                text_bytes: 1,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(
                run_formula(
                    &book,
                    0,
                    CellAddress { row: 0, col: 3 },
                    formula,
                    Default::default(),
                    budget
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    assert_eq!(
        evaluate(&book, "LEN(BASE(0,10,255.9))"),
        CellValue::Number(255.0)
    );
    assert_eq!(
        evaluate(&book, "IFERROR(BASE(1,2,1E99),42)"),
        CellValue::Number(42.0)
    );
}

#[test]
fn radix_numeric_arguments_use_calculation_locale_and_currency() {
    let book = book();
    let configuration = poirot_eval::RunConfiguration::default().with_number_locale(
        poirot_parsing::number::NumberLocaleV2::new(",", Some("."), Some("€"), "%", ":").unwrap(),
    );
    for (formula, expected) in [
        (r#"BASE("€15,9",16,"3,9")"#, CellValue::Text("00F".into())),
        (r#"DECIMAL("FF","16,9")"#, CellValue::Number(255.0)),
        (r#"DECIMAL("15,9",16)"#, CellValue::Error(CellError::Num)),
    ] {
        match poirot_eval::run_formula_with_configuration(
            &book,
            0,
            CellAddress { row: 0, col: 3 },
            formula,
            &configuration,
            Default::default(),
        ) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            outcome => panic!("{formula}: {outcome:?}"),
        }
    }
}

#[test]
fn radix_arity_and_reference_selection_remain_lazy() {
    let mut book = book();
    book.sheet_mut(0)
        .unwrap()
        .set_formula(CellAddress { row: 1, col: 0 }, "1/0", CellValue::Blank)
        .unwrap();
    assert_eq!(
        evaluate(&book, "BASE(A1:A2,16)"),
        CellValue::Text("F".into())
    );
    assert_eq!(
        evaluate(&book, "DECIMAL(A1:A2,16)"),
        CellValue::Number(21.0)
    );
    for formula in ["BASE(1,2,3,4)", "DECIMAL(1,2,3)"] {
        match run_formula(
            &book,
            0,
            CellAddress { row: 0, col: 3 },
            formula,
            Default::default(),
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
        ) {
            EvaluationOutcome::Complete { value, .. } => {
                assert_eq!(value, CellValue::Error(CellError::Value))
            }
            outcome => panic!("{formula}: {outcome:?}"),
        }
    }
}
