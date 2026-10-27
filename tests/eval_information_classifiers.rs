//! Source-derived scalar LogicalFunction classification, not Excel observations.
use poirot::eval::{EvaluationBudgets, EvaluationError, EvaluationOutcome};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        ("A1", CellValue::Number(7.)),
        ("B1", CellValue::Text("text".into())),
        ("A2", CellValue::Boolean(true)),
        ("B2", CellValue::Error(CellError::NA)),
    ] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address, value)
            .unwrap();
    }
    workbook
}

fn execute(formula: &str, address: &str) -> EvaluationOutcome<CellValue> {
    poirot::eval::run_formula_with_configuration(
        &workbook(),
        0,
        address.parse::<CellAddress>().unwrap(),
        formula,
        &poirot::eval::RunConfiguration::default()
            .with_array_result_policy(poirot::eval::ArrayResultPolicy::ScalarIntersection),
        EvaluationBudgets::default(),
    )
}

fn expect(formula: &str, address: &str, expected: CellValue) {
    match execute(formula, address) {
        EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn missing_is_not_blank_or_an_error_and_remains_nontext() {
    // LogicalFunction checks ValueEval classes; MissingArgEval is its own class.
    for classifier in [
        "ISBLANK",
        "ISERROR",
        "ISERR",
        "ISNA",
        "ISNUMBER",
        "ISTEXT",
        "ISLOGICAL",
        "ISNONTEXT",
    ] {
        for operand in ["IFS(TRUE,)", "IFS(TRUE,IFS(TRUE,))"] {
            expect(
                &format!("{classifier}({operand})"),
                "C3",
                CellValue::Boolean(classifier == "ISNONTEXT"),
            );
        }
    }
    expect("ISBLANK(C1)", "C3", CellValue::Boolean(true));
    expect("ISBLANK(IFS(TRUE,C1))", "C3", CellValue::Boolean(true));
    expect("ISBLANK(IF(TRUE,,7))", "C3", CellValue::Boolean(true));
}

#[test]
fn scalar_classes_and_spreadsheet_errors_are_classified_without_coercion() {
    for (formula, expected) in [
        ("ISNUMBER(7)", true),
        ("ISNUMBER(\"7\")", false),
        ("ISTEXT(\"\")", true),
        ("ISLOGICAL(TRUE)", true),
        ("ISNONTEXT(NA())", true),
        ("ISERROR(1/0)", true),
        ("ISERR(1/0)", true),
        ("ISERR(NA())", false),
        ("ISNA(NA())", true),
        ("ISNUMBER(A1)", true),
        ("ISTEXT(B1)", true),
        ("ISLOGICAL(A2)", true),
        ("ISNA(B2)", true),
    ] {
        expect(formula, "C3", CellValue::Boolean(expected));
    }
}

#[test]
fn explicit_reference_intersection_retains_scalar_classification() {
    // @ selects the caller's row and column before the classifier consumes it.
    expect("ISNA(@B1:B2)", "C2", CellValue::Boolean(true));
    expect("ISNUMBER(@A1:A2)", "C1", CellValue::Boolean(true));
    expect("ISERROR(@A1:A2)", "C3", CellValue::Boolean(true));
    expect("ISNUMBER(@A1:A2)", "C3", CellValue::Boolean(false));
    expect(
        "ISBLANK(INDEX({1,\"\";3,4},1,2))",
        "B1",
        CellValue::Boolean(false),
    );
}

#[test]
fn arity_and_lazy_errors_do_not_turn_operational_failures_into_cell_errors() {
    // FormulaParser.Arguments returns zero arguments for (), distinct from
    // MissingArgEval returned by IFS(TRUE,). Fixed1ArgFunction rejects arity.
    for classifier in [
        "ISBLANK",
        "ISERROR",
        "ISERR",
        "ISNA",
        "ISNUMBER",
        "ISTEXT",
        "ISLOGICAL",
        "ISNONTEXT",
    ] {
        expect(
            &format!("{classifier}()"),
            "C3",
            CellValue::Error(CellError::Value),
        );
    }
    // FormulaParser.parseArrayItem requires a constant at each array position;
    // {1,;3,4} is invalid syntax, so it cannot exercise Missing classification.
    for formula in [
        "ISBLANK(1,2)",
        "ISERROR(ISEVEN(IFS(TRUE,)),2)",
        "ISBLANK({1,;3,4})",
    ] {
        expect(formula, "C3", CellValue::Error(CellError::Value));
    }
    expect("IFERROR(ISERROR(1/0),42)", "C3", CellValue::Boolean(true));
    expect(
        "IFERROR(ISBLANK(IFS(TRUE,)),42)",
        "C3",
        CellValue::Boolean(false),
    );
    expect(
        "IF(FALSE,ISERROR(ISEVEN(IFS(TRUE,))),7)",
        "C3",
        CellValue::Number(7.),
    );
    {
        let formula = "IFERROR(IFS(TRUE,),42)";
        assert!(
            matches!(
                execute(formula, "C3"),
                EvaluationOutcome::Aborted {
                    failure: EvaluationError::UnsupportedCapability { .. },
                    ..
                }
            ),
            "{formula}"
        );
    }
}
