use poirot::eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
use std::time::{Duration, UNIX_EPOCH};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, 10.)
        .unwrap();
    workbook
}
const AT: CellAddress = CellAddress { row: 1, col: 1 };

#[test]
fn documented_engineering_values_types_and_domains() {
    use CellValue::{Error, Number, Text};
    // Microsoft engineering syntax: radix digits, complex coefficient extraction and numeric DELTA.
    // https://support.microsoft.com/en-us/excel/functions/bin2dec-function
    // https://support.microsoft.com/en-us/excel/functions/complex-function
    for (formula, expected) in [
        ("BIN2DEC(A1)", Number(2.)),
        ("HEX2DEC(\"FFFFFFFFFF\")", Number(-1.)),
        ("OCT2DEC(\"7777777777\")", Number(-1.)),
        ("DEC2BIN(5.9,4.9)", Text("0101".into())),
        ("DEC2HEX(255.9,4)", Text("00FF".into())),
        ("COMPLEX(3,2,\"j\")", Text("3+2j".into())),
        ("IMAGINARY(\"2+3i\")", Number(3.)),
        ("IMREAL(\"2+3i\")", Number(2.)),
        (r#"DELTA(" +.5e1 ",5)"#, Number(1.)),
        ("BESSELJ(0,0)", Number(1.)),
        ("BIN2DEC(2)", Error(CellError::Num)),
        ("BIN2DEC(\"１２\")", Error(CellError::Num)),
        ("BIN2DEC(\"x+00000001\")", Error(CellError::Num)),
        ("BIN2DEC(\"1000000002\")", Error(CellError::Num)),
        ("DEC2BIN(512)", Error(CellError::Num)),
        ("DEC2BIN(5,2)", Error(CellError::Num)),
        ("DELTA(\"0x1p0\",1)", Error(CellError::Value)),
        ("DELTA(\"NaN\",1)", Error(CellError::Value)),
        ("COMPLEX(1,2,\"I\")", Error(CellError::Value)),
    ] {
        let EvaluationOutcome::Complete { value, .. } = modern_formula(
            &workbook(),
            0,
            AT,
            formula,
            Default::default(),
            Default::default(),
        ) else {
            panic!("{formula}")
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn invalid_arity_and_catchable_radix_errors_remain_explicit() {
    for (formula, expected) in [
        (
            "DELTA(UNKNOWN_FUNCTION(),0,0)",
            CellValue::Error(CellError::Value),
        ),
        ("IFERROR(DEC2BIN(512),7)", CellValue::Number(7.)),
    ] {
        let EvaluationOutcome::Complete { value, .. } = modern_formula(
            &workbook(),
            0,
            AT,
            formula,
            Default::default(),
            Default::default(),
        ) else {
            panic!("{formula}")
        };
        assert_eq!(value, expected, "{formula}");
    }
}

#[test]
fn bessel_positive_order_and_formula_cell_errors() {
    let mut workbook = workbook();
    match match modern_formula(
        &workbook,
        0,
        AT,
        "BESSELJ(1.9,2)",
        Default::default(),
        Default::default(),
    ) {
        poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    } {
        CellValue::Number(value) => assert!((value - 0.329925727692387).abs() < 1e-14),
        value => panic!("unexpected Bessel result: {value:?}"),
    }
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 0 },
            "1/0",
            CellValue::Number(99.),
        )
        .unwrap();
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            "HEX2DEC(A1)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            "IFERROR(HEX2DEC(A1),7)",
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(7.)
    );
}

#[test]
fn operational_failures_survive_iferror() {
    let workbook = workbook();
    for (formula, budgets) in [
        (
            "IFERROR(BESSELJ(1,1000000),7)",
            EvaluationBudgets {
                run_work: 1000,
                ..EvaluationBudgets::default()
            },
        ),
        (
            "IFERROR(COMPLEX(3,2),7)",
            EvaluationBudgets {
                text_bytes: 100,
                ..EvaluationBudgets::default()
            },
        ),
    ] {
        assert!(
            matches!(
                modern_formula(
                    &workbook,
                    0,
                    AT,
                    formula,
                    EvaluationInputs::default(),
                    budgets
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    // OperandResolver rejects unparseable text with catchable VALUE, while
    // an unsupported frozen clock remains an operational DateParsing abort.
    assert!(matches!(
        modern_formula(
            &workbook,
            0,
            AT,
            r#"IFERROR(COMPLEX("not a date",2),7)"#,
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(7.),
            ..
        }
    ));
    assert!(matches!(
        modern_formula(
            &workbook,
            0,
            AT,
            r#"IFERROR(COMPLEX("1 Jan",2),7)"#,
            EvaluationInputs {
                as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
                ..EvaluationInputs::default()
            },
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

/// Run documented engineering controls under the modern builtin contract.
fn modern_formula(
    book: &Workbook,
    sheet: usize,
    at: CellAddress,
    source: &str,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    poirot::eval::run_formula_with_configuration(
        book,
        sheet,
        at,
        source,
        &poirot::eval::RunConfiguration::from(context)
            .with_builtin_policy(poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation),
        budgets,
    )
}
