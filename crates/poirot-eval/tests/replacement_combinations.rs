use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn book() -> Workbook {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    let s = book.sheet_mut(0).unwrap();
    s.set_value("A1".parse().unwrap(), "4").unwrap();
    s.set_value("A2".parse().unwrap(), true).unwrap();
    s.set_value("A3".parse().unwrap(), CellValue::Error(CellError::NA))
        .unwrap();
    book
}
fn run(
    formula: &str,
    config: &RunConfiguration,
    budget: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        &book(),
        0,
        CellAddress { row: 0, col: 25 },
        formula,
        config,
        budget,
    )
}
fn value(formula: &str, config: &RunConfiguration) -> CellValue {
    match run(formula, config, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
#[test]
fn combina_uses_one_documented_contract_and_preserves_scalar_admission() {
    for policy in [
        BuiltinSemanticsPolicy::PinnedPoi,
        BuiltinSemanticsPolicy::ExcelDocumentation,
    ] {
        let c = RunConfiguration::default().with_builtin_policy(policy);
        for (formula, n) in [
            ("COMBINA(4.23,3)", 20.),
            ("COMBINA(10,3)", 220.),
            ("COMBINA(3.2,3.8)", 10.),
            ("COMBINA(0,0)", 0.),
            ("COMBINA(,0)", 0.),
            ("COMBINA(4,)", 1.),
            ("COMBINA(4,A4)", 1.),
            ("COMBINA(4,TRUE)", 4.),
            ("COMBINA(4,A2)", 4.),
            ("COMBINA(4,FALSE)", 1.),
            ("COMBINA(A1,3)", 20.),
            ("COMBINA(\"4\",3)", 20.),
            ("COMBINA(1E308,1)", 1e308),
        ] {
            assert_eq!(value(formula, &c), CellValue::Number(n), "{formula}");
        }
        for (formula, error) in [
            ("COMBINA(-0.2,0)", CellError::Num),
            ("COMBINA(1,2)", CellError::Num),
            ("COMBINA(1E308,1E308)", CellError::Num),
            ("COMBINA(1E308,2)", CellError::Num),
            ("COMBINA(\"x\",3)", CellError::Value),
            ("COMBINA(\"\",3)", CellError::Value),
            ("COMBINA(A3,UNKNOWN_FUNCTION())", CellError::NA),
            ("COMBINA(-1,1/0)", CellError::Div0),
            ("COMBINA(4)", CellError::Value),
            ("COMBINA(4,3,UNKNOWN_FUNCTION())", CellError::Value),
        ] {
            assert_eq!(value(formula, &c), CellValue::Error(error), "{formula}");
        }
    }
}
#[test]
fn combina_contextual_text_and_resource_failures_remain_distinct() {
    let c = RunConfiguration::default().with_number_locale(
        poirot_coercionv2::NumberLocaleV2::new(",", Some("."), Some("€"), "%", ":").unwrap(),
    );
    assert_eq!(
        value("COMBINA(\"€4,23\",\"3,8\")", &c),
        CellValue::Number(20.)
    );
    for (f, b) in [
        (
            "IFERROR(COMBINA(1000,200),0)",
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            },
        ),
        (
            "IFERROR(COMBINA(\"4444\",2),0)",
            EvaluationBudgets {
                text_bytes: 3,
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(run(f, &c, b), EvaluationOutcome::Aborted { .. }),
            "{f}"
        );
    }
}
