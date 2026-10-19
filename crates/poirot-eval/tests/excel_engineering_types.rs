use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationFailure, EvaluationOutcome,
    RunConfiguration, TextSemanticsPolicy, TextSourceVersion, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, ExcelText, Workbook};

fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        (
            "A1",
            CellValue::TextUtf16(ExcelText::from_units(vec![65, 0xd800])),
        ),
        ("A2", CellValue::Text("-2+3i".into())),
        ("A3", CellValue::Text("2".into())),
        ("A4", CellValue::Number(0.)),
        ("A5", CellValue::Error(CellError::NA)),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    w
}
fn run(
    formula: &str,
    config: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        &book(),
        0,
        CellAddress { row: 0, col: 25 },
        formula,
        config,
        budgets,
    )
}
fn value(formula: &str, config: &RunConfiguration) -> CellValue {
    match run(formula, config, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}
fn excel() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}

#[test]
fn numeric_results_flow_through_predicates_references_and_arithmetic() {
    use CellValue::{Boolean, Number};
    for (formula, expected) in [
        (r#"CODE("A")"#, Number(65.)),
        ("CODE(A1)", Number(65.)),
        ("ISNUMBER(CODE(A1))", Boolean(true)),
        ("ISTEXT(CODE(A1))", Boolean(false)),
        ("CODE(A1)+2", Number(67.)),
        ("IMAGINARY(A2)", Number(3.)),
        ("IMREAL(A2)", Number(-2.)),
        ("ISNUMBER(IMAGINARY(A2))", Boolean(true)),
        ("ISTEXT(IMREAL(A2))", Boolean(false)),
        ("SUM(IMAGINARY(A2),IMREAL(A2))", Number(1.)),
        ("IMREAL(2)", Number(2.)),
        ("DELTA(A4)", Number(1.)),
        ("DELTA(1)", Number(0.)),
        ("DELTA(0,0)", Number(1.)),
        ("DELTA(0,)", Number(1.)),
        ("DELTA(1,)", Number(0.)),
        ("BIN2DEC(\"1111111111\")", Number(-1.)),
    ] {
        assert_eq!(value(formula, &excel()), expected, "{formula}");
    }
}

#[test]
fn code_uses_exact_first_unit_independently_of_text_policy() {
    for source in [
        TextSourceVersion::Poi7321209,
        TextSourceVersion::Poi7321209OpenJdk21Unicode15,
        TextSourceVersion::ExcelCompatibilityV2Unverified,
        TextSourceVersion::ExcelCompatibilityV2Documented,
    ] {
        let config = excel().with_text_policy(TextSemanticsPolicy { source });
        assert_eq!(value("CODE(A1)", &config), CellValue::Number(65.));
        assert_eq!(value("CODE(123)", &config), CellValue::Number(49.));
        for formula in [r#"IFERROR(CODE("é"),7)"#, r#"CODE("😀")"#] {
            assert!(
                matches!(
                    run(formula, &config, EvaluationBudgets::default()),
                    EvaluationOutcome::Aborted {
                        failure: EvaluationFailure::UnsupportedCapability { .. },
                        ..
                    }
                ),
                "{source:?}: {formula}"
            );
        }
    }
}

#[test]
fn errors_and_invalid_arity_preserve_semantic_order() {
    use CellValue::Error;
    for (formula, expected) in [
        ("CODE(A5)", Error(CellError::NA)),
        (r#"CODE("")"#, Error(CellError::Value)),
        ("IMREAL(A5)", Error(CellError::NA)),
        ("IMAGINARY(A5)", Error(CellError::NA)),
        (r#"IMREAL("bad")"#, Error(CellError::Num)),
        ("DELTA(A5)", Error(CellError::NA)),
        (r#"DELTA("bad",A5)"#, Error(CellError::Value)),
        ("DELTA()", Error(CellError::Value)),
        ("DELTA(UNKNOWN_FUNCTION(),0,0)", Error(CellError::Value)),
        ("BIN2DEC(A3)", Error(CellError::Num)),
        (r#"BIN2DEC("１２")"#, Error(CellError::Num)),
        (r#"BIN2DEC("x+00000001")"#, Error(CellError::Num)),
        ("BIN2DEC(A5)", Error(CellError::NA)),
    ] {
        assert_eq!(value(formula, &excel()), expected, "{formula}");
    }
    // Only DELTA's optional second operand receives the documented zero default.
    for (formula, config) in [
        ("DELTA(,0)", excel()),
        ("DELTA(0,)", RunConfiguration::default()),
    ] {
        assert!(matches!(
            run(formula, &config, EvaluationBudgets::default()),
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::UnsupportedCapability { .. },
                ..
            }
        ));
    }
}

#[test]
fn pinned_defaults_keep_text_results_arity_and_permissive_digits() {
    use CellValue::{Error, Number, Text};
    let pinned = RunConfiguration::default();
    for (formula, expected) in [
        (r#"CODE("A")"#, Number(65.)),
        ("IMREAL(A2)", Text("-2".into())),
        ("IMAGINARY(A2)", Text("3".into())),
        ("DELTA(0)", Error(CellError::Value)),
        ("BIN2DEC(A3)", Number(2.)),
        (r#"BIN2DEC("１２")"#, Number(4.)),
    ] {
        assert_eq!(value(formula, &pinned), expected, "{formula}");
    }
    let poi_text = pinned.with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    });
    assert_eq!(value(r#"CODE("A")"#, &poi_text), Text("65".into()));
    assert_eq!(
        poirot_eval::registry::resolve_function(&poirot_parsing::formula::FunctionName::from(
            "DELTA"
        ))
        .unwrap()
        .arity
        .minimum,
        2
    );
}

#[test]
fn budgets_cannot_be_masked_by_iferror() {
    for formula in [
        "IFERROR(CODE(A1),7)",
        "IFERROR(IMREAL(A2),7)",
        "IFERROR(IMAGINARY(A2),7)",
        "IFERROR(DELTA(A4),7)",
        "IFERROR(BIN2DEC(A3),7)",
    ] {
        assert!(
            matches!(
                run(
                    formula,
                    &excel(),
                    EvaluationBudgets {
                        run_work: 1,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::Budget(_),
                    ..
                }
            ),
            "{formula}"
        );
    }
    assert!(matches!(
        run(
            "IFERROR(IMREAL(A2),7)",
            &excel(),
            EvaluationBudgets {
                text_bytes: 2,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}

#[test]
fn complex_components_preserve_sign_and_result_type() {
    use CellValue::{Boolean, Error, Number, Text};
    for (formula, expected) in [
        (r#"COMPLEX(3,-1)"#, Text("3-i".into())),
        (r#"COMPLEX(0,-1,"j")"#, Text("-j".into())),
        ("COMPLEX(0,0)", Text("0".into())),
        ("COMPLEX(-0,-0)", Text("0".into())),
        (r#"COMPLEX(2,3,"")"#, Text("2+3i".into())),
        (r#"COMPLEX(2,3,"I")"#, Error(CellError::Value)),
        (r#"IMREAL("-2i")"#, Number(0.0)),
        (r#"IMAGINARY("-2i")"#, Number(-2.0)),
        (r#"IMAGINARY("-j")"#, Number(-1.0)),
        (r#"IMREAL("1e-2-3e+2j")"#, Number(0.01)),
        (r#"IMAGINARY("1e-2-3e+2j")"#, Number(-300.0)),
        (r#"IMREAL(".5i")"#, Number(0.0)),
        (r#"IMAGINARY(".5i")"#, Number(0.5)),
        (r#"IMAGINARY(COMPLEX(3,-1))"#, Number(-1.0)),
        (r#"ISNUMBER(IMREAL(COMPLEX(0,0)))"#, Boolean(true)),
        (r#"IMREAL(INDEX({"-2i","3+j"},1,1))"#, Number(0.0)),
        (r#"IMAGINARY(INDEX({"-2i","3+j"},1,2))"#, Number(1.0)),
        (
            r#"IMREAL(INDEX({TRUE,"3+j"},1,1))"#,
            Error(CellError::Value),
        ),
        ("IMREAL(TRUE)", Error(CellError::Value)),
        ("IMAGINARY(FALSE)", Error(CellError::Value)),
        (r#"IMREAL("1,000")"#, Error(CellError::Num)),
        (r#"IMREAL("$2")"#, Error(CellError::Num)),
        (r#"IMREAL("1/2/2020")"#, Error(CellError::Num)),
        (r#"IMREAL("2 +3i")"#, Error(CellError::Num)),
        (r#"IMREAL("2+3I")"#, Error(CellError::Num)),
        (r#"IMREAL("")"#, Error(CellError::Num)),
        (r#"IMAGINARY("1e309i")"#, Error(CellError::Num)),
        (r#"IMAGINARY("😀i")"#, Error(CellError::Num)),
    ] {
        assert_eq!(value(formula, &excel()), expected, "{formula}");
    }
}

#[test]
fn complex_reference_origins_and_utf16_failures_preserve_admission() {
    let mut workbook = book();
    for (address, input) in [
        ("B1", CellValue::Text("-2i".into())),
        ("B2", CellValue::Boolean(true)),
        ("B3", CellValue::Blank),
        ("B4", CellValue::Text("".into())),
    ] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), input)
            .unwrap();
    }
    for (formula, expected) in [
        ("IMREAL(B1)", CellValue::Number(0.0)),
        ("IMAGINARY(B1)", CellValue::Number(-2.0)),
        ("IMREAL(B2)", CellValue::Error(CellError::Value)),
        ("IMREAL(B3)", CellValue::Number(0.0)),
        ("IMREAL(B4)", CellValue::Error(CellError::Num)),
    ] {
        match run_formula_with_configuration(
            &workbook,
            0,
            CellAddress { row: 0, col: 25 },
            formula,
            &excel(),
            EvaluationBudgets::default(),
        ) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            other => panic!("{formula}: {other:?}"),
        }
    }
    // A malformed UTF-16 scalar remains an operational host projection failure.
    for formula in ["IFERROR(IMREAL(A1),7)", "IFERROR(IMAGINARY(A1),7)"] {
        assert!(
            matches!(
                run(formula, &excel(), EvaluationBudgets::default()),
                EvaluationOutcome::Aborted {
                    failure: EvaluationFailure::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}
