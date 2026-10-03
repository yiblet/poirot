use poirot_eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome, run_formula};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
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
fn pinned_engineering_values_types_and_coercion() {
    use CellValue::{Error, Number, Text};
    for (formula, expected) in [
        ("BIN2DEC(A1)", Number(2.)),
        // Array scalar selection uses the caller's column: evaluating
        // BIN2DEC({10,11}) at A1 yields 2, at B1 yields 3, and at J10 yields VALUE.
        ("BIN2DEC({10,11})", Number(3.)),
        ("BIN2DEC(2)", Number(2.)),
        ("BIN2DEC(2000000000)", Number(-512.)),
        ("HEX2DEC(\"FFFFFFFFFF\")", Number(-1.)),
        ("OCT2DEC(\"7777777777\")", Number(-1.)),
        ("DEC2BIN(5.9,4.9)", Text("0101".into())),
        ("DEC2HEX(255.9,4)", Text("00FF".into())),
        ("COMPLEX(3,-1)", Text("3i".into())),
        ("COMPLEX(0,0)", Text("".into())),
        ("COMPLEX(3,2,\"j\")", Text("3+2j".into())),
        // Retained normal catalog cases have string results in pinned POI.
        (r#"IMAGINARY("2+3i")"#, Text("3".into())),
        (r#"IMREAL("2+3i")"#, Text("2".into())),
        ("IMAGINARY(\"2i\")", Text("1".into())),
        ("IMREAL(\"2i\")", Text("2".into())),
        ("DELTA(\" +.5e1 \",5)", Number(1.)),
        ("BESSELJ(0,0)", Number(1.)),
        ("BIN2DEC(\"1000000002\")", Error(CellError::Num)),
        ("DEC2BIN(512)", Error(CellError::Num)),
        ("DEC2BIN(5,2)", Error(CellError::Num)),
        ("DELTA(\"0x1p0\",1)", Error(CellError::Value)),
        ("DELTA(\"1d\",1)", Error(CellError::Value)),
        ("DELTA(\"NaN\",1)", Error(CellError::Value)),
        ("DELTA(\"1e400\",1)", Error(CellError::Value)),
        ("COMPLEX(1,2,\"I\")", Error(CellError::Value)),
        ("IMREAL(\"1e2i\")", Error(CellError::Num)),
        ("BESSELJ(-1,2)", Error(CellError::Num)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook(),
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn unused_places_and_arity_do_not_execute_arguments() {
    for (formula, expected) in [
        (
            "DEC2BIN(-1,UNKNOWN_FUNCTION())",
            CellValue::Text("1111111111".into()),
        ),
        ("DEC2HEX(-1,1/0)", CellValue::Text("FFFFFFFFFF".into())),
        (
            "DEC2BIN(512,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Num),
        ),
        (
            "DELTA(UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
        ("IFERROR(DEC2BIN(512),7)", CellValue::Number(7.)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook(),
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn bessel_positive_order_and_formula_cell_errors() {
    let mut workbook = workbook();
    match match poirot_eval::run_formula(
        &workbook,
        0,
        AT,
        "BESSELJ(1.9,2)",
        Default::default(),
        Default::default(),
    ) {
        poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    } {
        CellValue::Number(value) => assert!((value - 0.329925727692387).abs() < 1e-14),
        value => panic!("unexpected Bessel result: {value:?}"),
    }
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_formula(
            CellAddress { row: 0, col: 0 },
            "1/0",
            CellValue::Number(99.),
        )
        .unwrap();
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            AT,
            "HEX2DEC(A1)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        match poirot_eval::run_formula(
            &workbook,
            0,
            AT,
            "IFERROR(HEX2DEC(A1),7)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
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
        ("IFERROR(DEC2BIN(,2),7)", EvaluationBudgets::default()),
    ] {
        assert!(
            matches!(
                run_formula(
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
        run_formula(
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
        run_formula(
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

#[test]
fn binary_utf16_digit_paths_reach_the_actual_engineering_dispatch() {
    // Bin2Dec.java: String.length, Integer.parseInt per char, and radix-two
    // Long.parseLong in toggleBits. These are pinned POI expectations.
    let mut book = workbook();
    book.sheet_mut(0)
        .unwrap()
        .set_value(CellAddress { row: 0, col: 0 }, "１２３")
        .unwrap();
    for (formula, expected) in [
        ("BIN2DEC(A1)", CellValue::Number(11.0)),
        ("BIN2DEC(\"١٠\")", CellValue::Number(2.0)),
        ("BIN2DEC(\"１１１１１１１１１１\")", CellValue::Number(-1.0)),
        (
            "BIN2DEC(\"０００００００００１\")",
            CellValue::Number(-511.0),
        ),
        ("BIN2DEC(\"x+00000001\")", CellValue::Number(-511.0)),
        ("BIN2DEC(\"𝟙\")", CellValue::Error(CellError::Num)),
        ("HEX2DEC(\"１２\")", CellValue::Error(CellError::Num)),
        ("DELTA(\"١\",1)", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &book,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn engineering_source_error_precedence_and_fractional_ranges() {
    for (formula, expected) in [
        ("DELTA(\"bad\",1/0)", CellValue::Error(CellError::Value)),
        ("BESSELJ(\"bad\",1/0)", CellValue::Error(CellError::Value)),
        ("COMPLEX(1/0,\"bad\")", CellValue::Error(CellError::Div0)),
        ("DEC2BIN(-512.9,1/0)", CellValue::Text("1000000000".into())),
        (
            "DEC2HEX(-549755813888.5,1/0)",
            CellValue::Text("8000000000".into()),
        ),
        ("DEC2BIN(-0.9,1/0)", CellValue::Error(CellError::Div0)),
        ("DELTA(1)", CellValue::Error(CellError::Value)),
    ] {
        assert_eq!(
            match poirot_eval::run_formula(
                &workbook(),
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}
