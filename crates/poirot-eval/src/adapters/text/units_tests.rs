use super::*;
use poirot_parsing::formula::parse_formula;
fn evaluate(formula: &str, budgets: EvaluationBudgets) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.budget = BudgetMeter::new(budgets).unwrap();
    session.builtin_policy = BuiltinSemanticsPolicy::ExcelDocumentation;
    session.text_policy = TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209OpenJdk21Unicode15,
    };
    let at = "A1".parse().unwrap();
    let expr = bound::bind(&parse_formula(formula).unwrap());
    let v = session.expr(0, at, &expr)?;
    session.select_legacy_scalar(at, v)
}
#[test]
fn captured_unit_results_flow_through_handlers() {
    for (formula, expected) in [
        ("LEFT(\"😀\",1)", vec![0xd83d]),
        ("RIGHT(\"😀\",1)", vec![0xde00]),
        ("MID(\"😀\",2,1)", vec![0xde00]),
        (
            "CONCAT(LEFT(\"😀\",1),RIGHT(\"😀\",1))",
            vec![0xd83d, 0xde00],
        ),
        ("REPT(LEFT(\"😀\",1),2)", vec![0xd83d, 0xd83d]),
        ("UPPER(LEFT(\"😀\",1))", vec![0xd83d]),
    ] {
        let value = evaluate(formula, EvaluationBudgets::default()).unwrap();
        assert_eq!(
            value.text_ref().unwrap().units().collect::<Vec<_>>(),
            expected,
            "{formula}"
        );
    }
}
#[test]
fn source_search_and_ascii_cases_flow_through_handlers() {
    for (formula, expected) in [
        ("LEN(\"😀\")", Number(2.)),
        ("CODE(\"A\")", Number(65.)),
        ("CODE(\"\")", Error(VALUE_ERROR)),
        ("FIND(\"Z\",\"😀Z\")", Number(3.)),
        ("SEARCH(\"*\",\"a*b\")", Number(1.)),
        ("SEARCH(\"ß\",\"Straße\")", Number(5.)),
        ("SEARCH(\"Z\",\"ßZ\")", Number(2.)),
        ("SEARCH(\"\",\"abc\",100)", Error(VALUE_ERROR)),
        ("LOWER(\"ASCII\")", Text("ascii".into())),
        ("PROPER(\"aBC 12hELLO\")", Text("Abc 12Hello".into())),
        ("LEFT(\"abc\",2147483648)", Error(VALUE_ERROR)),
        ("FIND(\"x\",\"x\",2147483648)", Error(VALUE_ERROR)),
        ("UPPER(\"ascii\")", Text("ASCII".into())),
        ("REPT(\"😀\",16384)", Error(VALUE_ERROR)),
        ("REPT(\"x\",#N/A)", Error(NA)),
        ("LEFT(#N/A,#REF!)", Error(NA)),
        ("IF(FALSE,LEFT(\"😀\",1),\"ok\")", Text("ok".into())),
    ] {
        assert_eq!(
            evaluate(formula, EvaluationBudgets::default()).unwrap(),
            expected,
            "{formula}"
        );
    }
}
#[test]
fn budget_failures_are_operational_and_cannot_be_caught() {
    let budgets = EvaluationBudgets {
        text_bytes: 8,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        evaluate("IFERROR(UPPER(\"abcdefgh\"),\"caught\")", budgets),
        Err(EvaluationFailure::Budget(_))
    ));
    assert_eq!(
        evaluate("REPT(\"\",100000)", EvaluationBudgets::default()).unwrap(),
        Text(String::new())
    );
    // Unicode final sigma now supports punctuation and supplementary contexts.
    assert_eq!(
        evaluate(
            "IFERROR(LOWER(\"Σ😀\"),\"caught\")",
            EvaluationBudgets::default()
        )
        .unwrap(),
        CellValue::Text("σ😀".into())
    );
}

fn evaluate_source(
    formula: &str,
    source: TextSourceVersion,
    input: Vec<u16>,
    budgets: EvaluationBudgets,
) -> EvalResult<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), shared::value(input))
        .unwrap();
    let mut session = Session::with_context(&workbook, EvaluationInputs::default());
    session.text_policy = TextSemanticsPolicy { source };
    session.budget = BudgetMeter::new(budgets).unwrap();
    let at = "B1".parse().unwrap();
    let result = session.expr(0, at, &bound::bind(&parse_formula(formula).unwrap()))?;
    session.select_legacy_scalar(at, result)
}
#[test]
fn documented_v2_scans_and_replacement_preserve_exact_units() {
    let source = TextSourceVersion::ExcelCompatibilityV2Documented;
    let input = vec![0xd800, 0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f];
    for (formula, expected) in [
        ("LEFT(A1,2)", vec![0xd800, 0xd83d, 0xde00]),
        ("MID(A1,2.9,1.9)", vec![0xd83d, 0xde00]),
        ("RIGHT(A1,3)", vec![0xdc00, 0x301, 0xfe0f]),
        ("RIGHT(A1,4)", vec![0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f]),
        (
            "REPLACE(A1,2.9,1.9,\"X\")",
            vec![0xd800, 88, 0xdc00, 0x301, 0xfe0f],
        ),
        (
            "REPLACE(A1,99,1,\"X\")",
            vec![0xd800, 0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f, 88],
        ),
        (
            "REPLACE(A1,2,0,\"X\")",
            vec![0xd800, 88, 0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f],
        ),
        (
            "MID(A1,2,2147483647)",
            vec![0xd83d, 0xde00, 0xdc00, 0x301, 0xfe0f],
        ),
        ("MID(A1,99,1)", vec![]),
    ] {
        let result =
            evaluate_source(formula, source, input.clone(), EvaluationBudgets::default()).unwrap();
        assert_eq!(
            result.text_ref().unwrap().units().collect::<Vec<_>>(),
            expected,
            "{formula}"
        );
    }
    assert_eq!(
        evaluate_source(
            "LEN(A1)",
            source,
            input.clone(),
            EvaluationBudgets::default()
        )
        .unwrap(),
        Number(5.)
    );
    for formula in [
        "LEFT(A1,-0.1)",
        "MID(A1,0.9,1)",
        "MID(A1,1,-0.1)",
        "REPLACE(A1,0.9,1,\"X\")",
        "REPLACE(A1,1,-0.1,\"X\")",
    ] {
        assert_eq!(
            evaluate_source(formula, source, input.clone(), EvaluationBudgets::default()).unwrap(),
            Error(VALUE_ERROR),
            "{formula}"
        );
    }
    let result = evaluate_source(
        "REPLACE(A1,2,1,LEFT(\"😀\",1))",
        TextSourceVersion::Poi7321209,
        vec![0xd83d, 0xde00],
        EvaluationBudgets::default(),
    )
    .unwrap();
    assert_eq!(
        result.text_ref().unwrap().units().collect::<Vec<_>>(),
        vec![0xd83d, 0xd83d]
    );
    assert_eq!(
        evaluate_source(
            "LOWER(A1)",
            source,
            "😀ASCII".encode_utf16().collect(),
            EvaluationBudgets::default()
        )
        .unwrap(),
        Text("😀ascii".into())
    );
}
#[test]
fn documented_v2_scans_and_replacement_admit_work_and_storage() {
    for formula in ["IFERROR(LEN(A1),7)", "IFERROR(REPLACE(A1,1,1,\"X\"),7)"] {
        assert!(matches!(
            evaluate_source(
                formula,
                TextSourceVersion::ExcelCompatibilityV2Documented,
                vec![65; 100],
                EvaluationBudgets {
                    cell_work: 50,
                    run_work: 50,
                    ..EvaluationBudgets::default()
                }
            ),
            Err(EvaluationFailure::Budget(_))
        ));
    }
    assert!(matches!(
        evaluate_source(
            "IFERROR(REPLACE(A1,1,0,A1),7)",
            TextSourceVersion::Poi7321209,
            vec![65; 100],
            EvaluationBudgets {
                text_bytes: 250,
                ..EvaluationBudgets::default()
            }
        ),
        Err(EvaluationFailure::Budget(_))
    ));
}
