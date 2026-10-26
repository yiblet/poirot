use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    TextSemanticsPolicy, TextSourceVersion, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn run(
    formula: &str,
    config: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    run_formula_with_configuration(
        &book,
        0,
        CellAddress { row: 0, col: 25 },
        formula,
        config,
        budgets,
    )
}
fn value(formula: &str, config: &RunConfiguration) -> CellValue {
    match run(formula, config, Default::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn search_validates_empty_pattern_and_text_domains() {
    // Microsoft's SEARCH/FIND remarks bound starts by text length; FIND explicitly
    // permits empty find_text at start_num. Calc confirms the nonempty-needle
    // fractional boundary, but disagrees with empty needles in nonempty text.
    for source in [
        TextSourceVersion::Poi7321209,
        TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    ] {
        let config = RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
            .with_text_policy(TextSemanticsPolicy { source });
        for function in ["SEARCH", "FIND"] {
            for args in [
                r#""","""#,
                r#""a","""#,
                r#""*","""#,
                r#""","abc",0"#,
                r#""","abc",0.9"#,
                r#""","abc",4"#,
                r#""","abc",4.9"#,
                r#""c","abc",4"#,
            ] {
                let formula = format!("{function}({args})");
                assert_eq!(
                    value(&formula, &config),
                    CellValue::Error(CellError::Value),
                    "{source:?}: {formula}"
                );
            }
            for (args, expected) in [
                (r#""","abc""#, 1.),
                (r#""","abc",3"#, 3.),
                (r#""","abc",3.9"#, 3.),
                (r#""c","abc",3.9"#, 3.),
                (r#""c","abc",1.9"#, 3.),
            ] {
                let formula = format!("{function}({args})");
                assert_eq!(
                    value(&formula, &config),
                    CellValue::Number(expected),
                    "{source:?}: {formula}"
                );
            }
            assert_eq!(
                value(&format!("{function}(NA(),1/0,0)"), &config),
                CellValue::Error(CellError::NA)
            );
            assert_eq!(
                value(&format!(r#"{function}("a",1/0,0)"#), &config),
                CellValue::Error(CellError::Div0)
            );
            assert_eq!(
                value(&format!(r#"{function}("a","abc",NA())"#), &config),
                CellValue::Error(CellError::NA)
            );
        }
        let missing = r#"SEARCH("","abc",)"#;
        assert_eq!(value(missing, &config), CellValue::Error(CellError::Value));
        assert!(matches!(
            run(
                r#"IFERROR(SEARCH("long","long text"),0)"#,
                &config,
                EvaluationBudgets {
                    run_work: 1,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
        let comma = config.clone().with_number_locale(
            poirot_parsing::number::NumberLocaleV2::new(",", Some("."), None, "%", ":").unwrap(),
        );
        assert_eq!(
            value(r#"FIND("c","abc","3,9")"#, &comma),
            CellValue::Number(3.)
        );
    }
}

#[test]
fn search_source_matching_and_unicode_contracts_survive_domain_validation() {
    {
        let poi = RunConfiguration::default()
            .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
            .with_text_policy(TextSemanticsPolicy {
                source: TextSourceVersion::Poi7321209OpenJdk21Unicode15,
            });
        assert_eq!(value(r#"SEARCH("*","abc")"#, &poi), CellValue::Number(1.));
        assert_eq!(value(r#"SEARCH("a~*","a*")"#, &poi), CellValue::Number(1.));
        assert_eq!(value(r#"FIND("x","😀x")"#, &poi), CellValue::Number(3.));
        assert_eq!(value(r#"SEARCH("x","😀x")"#, &poi), CellValue::Number(3.));
        assert_eq!(value(r#"SEARCH("x","ßx")"#, &poi), CellValue::Number(2.));
    }
}
