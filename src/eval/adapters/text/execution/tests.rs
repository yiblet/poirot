use crate::eval::compile;
use crate::eval::config::EvaluationInputs;
use crate::eval::limits::{BudgetMeter, EvalResult, EvaluationBudgets};
use crate::eval::session::{ResultProjection, Session};
use crate::eval::values::VALUE_ERROR;
use crate::eval::vm::ExpressionRunner;
use crate::model::CellError::NA;
use crate::model::CellValue::{Error, Number, Text};
use crate::model::{CellValue, Workbook};
use poirot_parsing::formula::parse_formula;

fn evaluate(formula: &str) -> EvalResult<CellValue> {
    evaluate_with_budgets(formula, EvaluationBudgets::default())
}

fn evaluate_with_budgets(formula: &str, budgets: EvaluationBudgets) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let mut session = Session::new(
        &workbook,
        EvaluationInputs::default(),
        BudgetMeter::new(budgets)?,
    );

    let value = ExpressionRunner::new(&mut session).expr(
        0,
        "A1".parse().unwrap(),
        &compile::bound::bind(&parse_formula(formula).unwrap()),
    )?;
    ResultProjection::new(&mut session).select_legacy_scalar("A1".parse().unwrap(), value)
}

#[test]
fn formulas_select_units_without_normalizing_text() {
    assert_eq!(evaluate("LEN(\"A😀é\")").unwrap(), Number(4.));
    assert_eq!(evaluate("RIGHT(\"A😀é\",2)").unwrap(), Text("é".into()));
    assert_eq!(evaluate("LEFT(\"😀\",1)").unwrap(), Text("😀".into()));
}

#[test]
fn concatenation_uses_spreadsheet_numeric_text_without_rounding_values() {
    {
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
                    evaluate(&formula).unwrap(),
                    Text(format!("T{expected}")),
                    "{formula}"
                );
            }
        }
        assert_eq!(
            evaluate("(-0.02-0.02-0.1)*100").unwrap(),
            Number(-14.000000000000002)
        );
        assert_eq!(
            evaluate("\"T\"&TEXT(1.25,\"0.000\")").unwrap(),
            Text("T1.250".into())
        );
        assert_eq!(
            evaluate("\"T\"&\"-14.000000000000002\"").unwrap(),
            Text("T-14.000000000000002".into())
        );
    }
}

#[test]
fn source_concatenation_ceiling_is_independent_of_host_budget() {
    let operand = "😀".repeat(8192);
    let operator = format!("\"{operand}\"&\"\"");
    assert_eq!(evaluate(&operator).unwrap(), Text(operand.clone()));
    assert_eq!(evaluate(&operator).unwrap(), Text(operand.clone()));
    let function = format!("CONCATENATE(\"{operand}\")");
    assert_eq!(evaluate(&function).unwrap(), Text(operand));
    assert_eq!(
        evaluate("MID(\"abc\",2,2147483647)").unwrap(),
        Text("bc".into())
    );
}

#[test]
fn text_integer_admission_precedes_empty_text_and_index_clamping() {
    for formula in [
        "LEFT(\"\",2147483648)",
        "RIGHT(\"abc\",2147483648)",
        "MID(\"abc\",2147483648,1)",
        "MID(\"abc\",1,2147483648)",
        "REPLACE(\"abc\",1,2147483648,\"x\")",
        "MID(\"abc\",1,\"2147483648\")",
    ] {
        assert_eq!(evaluate(formula).unwrap(), Error(VALUE_ERROR), "{formula}");
    }
    for (formula, expected) in [
        ("LEFT(\"abc\",2147483647)", "abc"),
        ("LEFT(\"abc\",2147483647.5)", "abc"),
        ("MID(\"abc\",1.9,2.9)", "ab"),
    ] {
        assert_eq!(
            evaluate(formula).unwrap(),
            Text(expected.into()),
            "{formula}"
        );
    }
    assert_eq!(
        evaluate("REPT(\"\",2147483648)").unwrap(),
        Error(VALUE_ERROR)
    );
    assert_eq!(
        evaluate("REPT(\"\",2147483647)").unwrap(),
        Text(String::new())
    );
    // Range admission must not move a function's negative-start validation ahead
    // of conversion of the following count argument.
    assert_eq!(evaluate("MID(\"abc\",-1,#N/A)").unwrap(), Error(NA));
}
