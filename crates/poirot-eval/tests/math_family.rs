use poirot_eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
const AT: CellAddress = CellAddress { row: 1, col: 2 };

#[test]
fn math_formulas_preserve_rounding_and_source_error_order() {
    let workbook = workbook();
    for (formula, expected) in [
        ("FACT(2.9999999999999996)", CellValue::Number(6.)),
        ("QUOTIENT(880000000*0.00849,3)", CellValue::Number(2490400.)),
        ("CEILING.MATH(-4.3,2,-1)", CellValue::Number(-6.)),
        ("CEILING.MATH(-4.3,2,1)", CellValue::Number(-6.)),
        ("FLOOR.MATH(-4.3,2,-1)", CellValue::Number(-4.)),
        ("MROUND(0.45,0.1*3)", CellValue::Number(0.6)),
        ("ATAN2(0,0)", CellValue::Error(CellError::Div0)),
        ("GCD(1/0,2)", CellValue::Error(CellError::Div0)),
        ("GCD(-1,1/0)", CellValue::Error(CellError::Num)),
        ("QUOTIENT(1/0,2)", CellValue::Error(CellError::Div0)),
        ("FACT(1/0)", CellValue::Error(CellError::Div0)),
        (
            "CEILING.PRECISE(4.3,2,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
        (
            "FACT(UNKNOWN_FUNCTION(),2)",
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(
            match math_formula(
                &workbook,
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
fn scalar_reference_intersection_reads_only_selected_cell() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet
        .set_formula_with_cached_value(
            CellAddress { row: 0, col: 0 },
            "UNKNOWN_FUNCTION()",
            CellValue::Number(0.),
        )
        .unwrap();
    sheet.set_value(CellAddress { row: 1, col: 0 }, 5.).unwrap();
    sheet
        .set_formula_with_cached_value(
            CellAddress { row: 2, col: 0 },
            "UNKNOWN_FUNCTION()",
            CellValue::Number(0.),
        )
        .unwrap();
    for (formula, expected) in [("FACT(@A1:A3)", 120.), ("POWER(@A1:A3,2)", 25.)] {
        assert_eq!(
            match math_formula(
                &workbook,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            CellValue::Number(expected)
        );
    }
    assert_eq!(
        match math_formula(
            &workbook,
            0,
            AT,
            "FACT(B2)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(1.)
    );
    assert_eq!(
        match math_formula(
            &workbook,
            0,
            AT,
            "GCD({24;18},6)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(6.)
    );
}

#[test]
fn helper_budget_abort_survives_iferror() {
    let workbook = workbook();
    assert!(matches!(
        math_formula(
            &workbook,
            0,
            AT,
            "IFERROR(FACT(170),0)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: 100,
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn exact_binary_operands_reach_excel_arithmetic_through_references() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    // The workbook carries the exact operand bits independently of formula text.
    sheet
        .set_value(
            CellAddress { row: 0, col: 0 },
            f64::from_bits(0x4007FFFFFFFFFFFF),
        )
        .unwrap();
    sheet
        .set_value(CellAddress { row: 0, col: 1 }, 0.1f64 * 3.)
        .unwrap();
    sheet
        .set_value(
            CellAddress { row: 0, col: 2 },
            f64::from_bits(0x0036319916D67853),
        )
        .unwrap();
    assert_eq!(
        match math_formula(
            &workbook,
            0,
            AT,
            "FACT(A1)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(6.)
    );
    assert_eq!(
        match math_formula(
            &workbook,
            0,
            AT,
            "CEILING.PRECISE(B1,0.1)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(0.3)
    );
    assert_eq!(
        match math_formula(
            &workbook,
            0,
            AT,
            "CEILING.PRECISE(C1,C1)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(1.2345678901235e-307)
    );
}

#[test]
fn lcm_enforces_documented_output_limits() {
    // Microsoft documents #NUM! when the least common multiple is >= 2^53.
    // https://support.microsoft.com/en-us/excel/functions/lcm-function
    let workbook = workbook();
    for (formula, expected) in [
        ("LCM(1E30)", CellValue::Error(CellError::Num)),
        ("LCM(1E16,0)", CellValue::Number(0.)),
        ("LCM(1E30,0)", CellValue::Number(0.)),
        ("LCM(1E16,1,0)", CellValue::Error(CellError::Num)),
        ("LCM(1E16,0,1/0)", CellValue::Error(CellError::Div0)),
        ("LCM(1E16,0,-1)", CellValue::Error(CellError::Num)),
    ] {
        assert_eq!(
            match math_formula(
                &workbook,
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
fn mround_sign_validation_uses_source_floating_product() {
    let workbook = workbook();
    for (formula, expected) in [
        ("MROUND(-1E-300,1E-300)", CellValue::Number(-1e-300)),
        ("MROUND(1E-300,-1E-300)", CellValue::Number(1e-300)),
        ("MROUND(-1E-100,1E-100)", CellValue::Error(CellError::Num)),
    ] {
        assert_eq!(
            match math_formula(
                &workbook,
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
fn seriessum_preserves_term_domains_and_coefficient_order() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    sheet.set_value(CellAddress { row: 0, col: 0 }, 1.).unwrap();
    sheet.set_value(CellAddress { row: 2, col: 0 }, 3.).unwrap();
    sheet
        .set_value(CellAddress { row: 0, col: 1 }, CellValue::Text("2".into()))
        .unwrap();
    for (formula, expected) in [
        ("SERIESSUM(2,3,4,5)", CellValue::Number(40.)),
        ("SERIESSUM(2,2,1,{1,2,3;4,5,6})", CellValue::Number(1284.)),
        ("SERIESSUM(2,0,1,A1:A3)", CellValue::Number(13.)),
        (
            "SERIESSUM(2,0,1,{1,\"2\",3})",
            CellValue::Error(CellError::Value),
        ),
        (
            "SERIESSUM(2,0,1,{1,TRUE,3})",
            CellValue::Error(CellError::Value),
        ),
        ("SERIESSUM(2,0,1,\"2\")", CellValue::Number(2.)),
        ("SERIESSUM(2,0,1,B1)", CellValue::Number(2.)),
        ("SERIESSUM(2,0,1,B1:B1)", CellValue::Number(2.)),
        ("SERIESSUM(TRUE,0,1,1)", CellValue::Error(CellError::Value)),
        ("SERIESSUM(2,0,1,TRUE)", CellValue::Error(CellError::Value)),
        ("SERIESSUM(\"\",1,1,1)", CellValue::Error(CellError::Value)),
        ("SERIESSUM(2,0,1,)", CellValue::Error(CellError::Value)),
        ("SERIESSUM(0,0,1,1)", CellValue::Number(1.)),
        ("SERIESSUM(0,0,1,0)", CellValue::Number(0.)),
        ("SERIESSUM(0,-1,1,1)", CellValue::Error(CellError::Num)),
        ("SERIESSUM(0,-1,1,0)", CellValue::Error(CellError::Num)),
        ("SERIESSUM(0,0.5,1,1)", CellValue::Number(0.)),
        ("SERIESSUM(-4,0.5,1,0)", CellValue::Error(CellError::Num)),
        ("SERIESSUM(-4,2,-1,{1,2,3})", CellValue::Number(11.)),
        ("SERIESSUM(4,0.5,0.5,{1,2,3})", CellValue::Number(34.)),
        ("SERIESSUM(2,-1,-1,{2,4,8})", CellValue::Number(3.)),
        (
            "SERIESSUM(10,100,100,{1,2,3,4})",
            CellValue::Error(CellError::Num),
        ),
        (
            "SERIESSUM(1,0,0,{1E308,1E308,-1E308})",
            CellValue::Error(CellError::Num),
        ),
        ("SERIESSUM(1,0,0,{3,-2,-1})", CellValue::Number(0.)),
        ("SERIESSUM(1/0,NA(),0,1)", CellValue::Error(CellError::Div0)),
        (
            "SERIESSUM(0,-1,1,{1,#N/A})",
            CellValue::Error(CellError::Num),
        ),
        (
            "SERIESSUM(2,0,1,{#N/A,\"bad\"})",
            CellValue::Error(CellError::NA),
        ),
        (
            "SERIESSUM(UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(
            match math_formula(
                &workbook,
                0,
                AT,
                formula,
                Default::default(),
                Default::default()
            ) {
                EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn seriessum_reference_traversal_admission_aborts_outside_iferror() {
    let workbook = workbook();
    assert!(matches!(
        math_formula(
            &workbook,
            0,
            AT,
            "IFERROR(SERIESSUM(2,0,1,A1:A10000),0)",
            EvaluationInputs::default(),
            EvaluationBudgets {
                run_work: 100,
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn seriessum_scalar_text_uses_real_locale_parser() {
    let workbook = workbook();
    let configuration = poirot_eval::RunConfiguration::default().with_number_locale(
        poirot_parsing::number::NumberLocaleV2::new(",", Some("."), Some("€"), "%", ":").unwrap(),
    );
    for (formula, expected) in [
        (r#"SERIESSUM("€2,5",1,1,"4,0")"#, CellValue::Number(10.)),
        (
            r#"SERIESSUM(2,0,1,{"4,0"})"#,
            CellValue::Error(CellError::Value),
        ),
    ] {
        match poirot_eval::run_formula_with_configuration(
            &workbook,
            0,
            AT,
            formula,
            &configuration,
            Default::default(),
        ) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            outcome => panic!("{formula}: {outcome:?}"),
        }
    }
}

/// Run arithmetic controls with scalar presentation and the modern builtin defaults.
fn math_formula(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    inputs: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    poirot_eval::run_formula_with_configuration(
        workbook,
        sheet,
        address,
        formula,
        &poirot_eval::RunConfiguration::from(inputs)
            .with_array_result_policy(poirot_eval::ArrayResultPolicy::ScalarIntersection),
        budgets,
    )
}

#[test]
fn classic_math_preserves_domains_coercion_and_source_error_order() {
    // Independently authored arithmetic controls; source-order/error rules follow
    // Apache POI NumericFunction at the pinned revision recorded in LICENSES/ACKNOWLEDGEMENTS:
    // https://github.com/apache/poi/blob/732120980140d5ed64b482c470e0b625cdb1ab15/poi/src/main/java/org/apache/poi/ss/formula/functions/NumericFunction.java
    // No upstream implementation or test cases copied.
    let workbook = workbook();
    for (formula, expected) in [
        ("POWER(0,0)", CellValue::Error(CellError::Num)),
        ("POWER(0,-2)", CellValue::Error(CellError::Div0)),
        ("POWER(-4,0.5)", CellValue::Error(CellError::Num)),
        ("POWER(2,10)", CellValue::Number(1024.)),
        ("POWER(\"3\",TRUE)", CellValue::Number(3.)),
        ("POWER(,2)", CellValue::Number(0.)),
        ("POWER(#N/A,#REF!)", CellValue::Error(CellError::NA)),
        ("POWER(#REF!,#N/A)", CellValue::Error(CellError::Ref)),
        ("POWER(\"bad\",#N/A)", CellValue::Error(CellError::Value)),
        ("POWER(0,#N/A)", CellValue::Error(CellError::NA)),
        ("FLOOR(0,0)", CellValue::Number(0.)),
        ("FLOOR(2,0)", CellValue::Error(CellError::Div0)),
        ("CEILING(0,#N/A)", CellValue::Error(CellError::NA)),
        ("LOG(-1,#N/A)", CellValue::Error(CellError::NA)),
        ("LOG(8,2)", CellValue::Number(3.)),
        ("LOG(100)", CellValue::Number(2.)),
        ("TRUNC(-3.75)", CellValue::Number(-3.)),
        ("TRUNC(-3.75,1)", CellValue::Number(-3.7)),
        ("MOD(-5,3)", CellValue::Number(1.)),
    ] {
        match math_formula(
            &workbook,
            0,
            AT,
            formula,
            Default::default(),
            Default::default(),
        ) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            outcome => panic!("{formula}: {outcome:?}"),
        }
    }
}

#[test]
fn classic_math_arity_and_coercion_control_argument_demand() {
    let workbook = workbook();
    // Arity is rejected before RAND is forced. Valid eager signatures prepare
    // all operand geometry, then preserve source-ordered spreadsheet errors.
    for (formula, random_draws, expected) in [
        ("POWER(RAND())", 0, CellValue::Error(CellError::Value)),
        ("PI(RAND())", 0, CellValue::Error(CellError::Value)),
        (
            "POWER(\"bad\",RAND())",
            1,
            CellValue::Error(CellError::Value),
        ),
        ("POWER(0,RAND()+1)", 1, CellValue::Number(0.)),
    ] {
        let outcome = math_formula(
            &workbook,
            0,
            AT,
            formula,
            Default::default(),
            Default::default(),
        );
        assert_eq!(
            outcome.usage().random_accesses.len(),
            random_draws,
            "{formula}"
        );
        match outcome {
            EvaluationOutcome::Complete { value, .. } => {
                assert_eq!(value, expected, "{formula}")
            }
            outcome => panic!("{formula}: {outcome:?}"),
        }
    }
    assert!(matches!(
        math_formula(
            &workbook,
            0,
            AT,
            "IFERROR(POWER(2,FACT(170)),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: poirot_eval::EvaluationFailure::Budget(_),
            ..
        }
    ));
}
