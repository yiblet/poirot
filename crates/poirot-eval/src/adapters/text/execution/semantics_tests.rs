use super::*;
use poirot_parsing::formula::parse_formula;
fn evaluate(source: TextSourceVersion, formula: &str) -> EvalResult<CellValue> {
    evaluate_with_budgets(source, formula, EvaluationBudgets::default())
}
fn evaluate_with_budgets(
    source: TextSourceVersion,
    formula: &str,
    budgets: EvaluationBudgets,
) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(budgets)?,
    );
    session.text_policy = TextSemanticsPolicy { source };
    session.builtin_policy = if matches!(
        source,
        TextSourceVersion::Poi7321209 | TextSourceVersion::Poi7321209OpenJdk21Unicode15
    ) {
        BuiltinSemanticsPolicy::PinnedPoi
    } else {
        BuiltinSemanticsPolicy::ExcelDocumentation
    };
    let value = session.expr(
        0,
        "A1".parse().unwrap(),
        &bound::bind(&parse_formula(formula).unwrap()),
    )?;
    session.select_legacy_scalar("A1".parse().unwrap(), value)
}
#[test]
fn formulas_select_units_without_normalizing_text() {
    for (source, expected) in [
        (TextSourceVersion::ExcelCompatibilityV2Documented, 4.),
        (TextSourceVersion::Poi7321209, 5.),
    ] {
        assert_eq!(evaluate(source, "LEN(\"A😀é\")").unwrap(), Number(expected));
        assert_eq!(
            evaluate(source, "RIGHT(\"A😀é\",2)").unwrap(),
            Text("é".into())
        );
    }
    assert_eq!(
        evaluate(
            TextSourceVersion::ExcelCompatibilityV2Documented,
            "LEFT(\"😀\",1)"
        )
        .unwrap(),
        Text("😀".into())
    );
    assert_eq!(
        evaluate(TextSourceVersion::Poi7321209, "LEFT(\"😀\",1)").unwrap(),
        CellValue::TextUtf16(poirot_model::ExcelText::from_units(vec![0xd83d]))
    );
    assert!(matches!(
        evaluate(
            TextSourceVersion::ExcelCompatibilityV2Unverified,
            "LEN(\"abc\")"
        ),
        Err(EvaluationFailure::UnsupportedCapability { .. })
    ));
}
#[test]
fn concatenation_uses_spreadsheet_numeric_text_without_rounding_values() {
    for source in [TextSourceVersion::Poi7321209, TextSourceVersion::Poi7321209] {
        for (expression, expected) in [
            ("(-0.02-0.02-0.1)*100", "-14"),
            ("123456789012345.6", "123456789012346"),
            ("1E21", "1E+21"),
            ("1E-19", "1E-19"),
            ("-0.0", "0"),
            ("1.7976931348623157E308", "1.7976931348623E+308"),
            ("2.2250738585072014E-308", "2.2250738585072E-308"),
        ] {
            for formula in [
                format!("\"T\"&({expression})"),
                format!("CONCAT(\"T\",{expression})"),
                format!("CONCATENATE(\"T\",{expression})"),
            ] {
                assert_eq!(
                    evaluate(source, &formula).unwrap(),
                    Text(format!("T{expected}")),
                    "{source:?}: {formula}"
                );
            }
        }
        assert_eq!(
            evaluate(source, "(-0.02-0.02-0.1)*100").unwrap(),
            Number(-14.000000000000002)
        );
        assert_eq!(
            evaluate(source, "\"T\"&TEXT(1.25,\"0.000\")").unwrap(),
            Text("T1.250".into())
        );
        assert_eq!(
            evaluate(source, "\"T\"&\"-14.000000000000002\"").unwrap(),
            Text("T-14.000000000000002".into())
        );
    }
}
#[test]
fn source_concatenation_ceiling_is_independent_of_host_budget() {
    let operand = "😀".repeat(8192);
    let operator = format!("\"{operand}\"&\"\"");
    assert_eq!(
        evaluate(TextSourceVersion::Poi7321209, &operator).unwrap(),
        Text(operand.clone())
    );
    assert_eq!(
        evaluate(TextSourceVersion::Poi7321209, &operator).unwrap(),
        Text(operand.clone())
    );
    let function = format!("CONCATENATE(\"{operand}\")");
    assert_eq!(
        evaluate(TextSourceVersion::Poi7321209, &function).unwrap(),
        Text(operand)
    );
    assert_eq!(
        evaluate(TextSourceVersion::Poi7321209, "MID(\"abc\",2,2147483647)").unwrap(),
        Text("bc".into())
    );
}

#[test]
fn text_integer_admission_precedes_empty_text_and_index_clamping() {
    let source = TextSourceVersion::Poi7321209;
    for formula in [
        "LEFT(\"\",2147483648)",
        "RIGHT(\"abc\",2147483648)",
        "MID(\"abc\",2147483648,1)",
        "MID(\"abc\",1,2147483648)",
        "REPLACE(\"abc\",1,2147483648,\"x\")",
        "MID(\"abc\",1,\"2147483648\")",
    ] {
        assert_eq!(
            evaluate(source, formula).unwrap(),
            Error(VALUE_ERROR),
            "{formula}"
        );
    }
    for (formula, expected) in [
        ("LEFT(\"abc\",2147483647)", "abc"),
        ("LEFT(\"abc\",2147483647.5)", "abc"),
        ("MID(\"abc\",1.9,2.9)", "ab"),
    ] {
        assert_eq!(
            evaluate(source, formula).unwrap(),
            Text(expected.into()),
            "{formula}"
        );
    }
    assert_eq!(
        evaluate(source, "REPT(\"\",2147483648)").unwrap(),
        Error(VALUE_ERROR)
    );
    assert_eq!(
        evaluate(source, "REPT(\"\",2147483647)").unwrap(),
        Text(String::new())
    );
    // Range admission must not move a function's negative-start validation ahead
    // of conversion of the following count argument.
    assert_eq!(evaluate(source, "MID(\"abc\",-1,#N/A)").unwrap(), Error(NA));
}
