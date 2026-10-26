use poirot_eval::{
    EvaluationBudgets, EvaluationInputs, EvaluationOutcome, bound::FormulaPlan, run_plan,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};

const AT: CellAddress = CellAddress { row: 0, col: 3 };
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
fn number(workbook: &Workbook, formula: &str, expected: f64) {
    let value = match modern_formula(
        workbook,
        0,
        AT,
        formula,
        Default::default(),
        Default::default(),
    ) {
        poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    let CellValue::Number(actual) = value else {
        panic!("{formula}: {value:?}")
    };
    assert!(
        (actual - expected).abs() <= 1e-10 * expected.abs().max(1.0),
        "{formula}: {actual} != {expected}"
    );
}

#[test]
fn zero_rate_annuities_and_documented_financial_domains() {
    let workbook = workbook();
    for (formula, expected) in [
        ("FV(0,10,-20,,)", 200.),
        ("PV(0,10,-20)", 200.),
        ("PMT(0,10,200)", -20.),
        ("NPER(0,-20,200)", 10.),
        ("IPMT(0,1,12,1000)", 0.),
        ("PPMT(0,1,12,1000)", -1000. / 12.),
        ("MIRR({-100,121},0.1,0.1)", 0.21),
        ("DOLLARDE(1.02,16)", 1.125),
        ("DOLLARFR(1.125,16)", 1.02),
    ] {
        number(&workbook, formula, expected);
    }
    for (formula, expected) in [
        ("MIRR({0,0},0.1,0.1)", CellError::Div0),
        ("MIRR({10,20},0.1,0.1)", CellError::Div0),
        ("MIRR({-10,-20},0.1,0.1)", CellError::Div0),
        ("MIRR(0.1,0.1)", CellError::Value),
        ("IPMT(0.1,1,12,1000,1/0)", CellError::Div0),
        ("DOLLARDE(TRUE,16)", CellError::Value),
        ("RATE(\"bad\",1,100)", CellError::Value),
    ] {
        assert!(
            matches!(modern_formula(&workbook, 0, AT, formula, Default::default(), Default::default()), EvaluationOutcome::Complete { value: CellValue::Error(error), .. } if error == expected),
            "{formula}"
        );
    }
}

#[test]
fn collections_preserve_origins_and_skip_non_numeric_periods() {
    let mut workbook = workbook();
    let sheet = workbook.sheet_mut(0).unwrap();
    for (row, value) in [
        (0, CellValue::Number(-100.)),
        (1, CellValue::Text("200".into())),
        (2, CellValue::Boolean(true)),
        (3, CellValue::Number(121.)),
    ] {
        sheet.set_value(CellAddress { row, col: 0 }, value).unwrap();
    }
    number(&workbook, "NPV(0.1,A1:A4)", 9.09090909090908);
    number(&workbook, "NPV(0,TRUE,\"2\")", 3.);
    number(&workbook, "NPV(0,{TRUE,\"2\",3})", 3.);
    // The future balance participates in the annuity payment; it is never an unused input.
    // https://support.microsoft.com/en-us/excel/functions/ppmt-function
    number(&workbook, "PPMT(0.1,1,2,100,50)", -71.42857142857143);
    assert!(matches!(
        modern_formula(
            &workbook,
            0,
            AT,
            "PPMT(0.1,1,2,100,1/0)",
            Default::default(),
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Div0),
            ..
        }
    ));
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            "FIXED(1234.5,2,\"TRUE\")",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Text("1234.50".into())
    );
}

#[test]
fn resource_and_capability_faults_survive_iferror() {
    let workbook = workbook();
    for (formula, budgets) in [
        (
            "IFERROR(IRR({-100,110}),99)",
            EvaluationBudgets {
                run_work: 100,
                ..EvaluationBudgets::default()
            },
        ),
        (
            "IFERROR(FIXED(1),99)",
            EvaluationBudgets {
                materialized_values: 100,
                ..EvaluationBudgets::default()
            },
        ),
    ] {
        let plan =
            FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
        assert!(
            matches!(
                run_plan(
                    &workbook,
                    0,
                    AT,
                    &plan,
                    EvaluationInputs::default(),
                    budgets
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    // OperandResolver's fpPattern excludes Java hexadecimal floating strings.
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            r#"PMT("0x1p0",2,1)"#,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            r#"IFERROR(PMT("0x1p0",2,1),99)"#,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(99.)
    );
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            "IFERROR(IPMT(0,1,12,1000),99)",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(0.)
    );
}

#[test]
fn financial_decimal_strings_and_direct_cashflow_admission() {
    let workbook = workbook();
    for formula in [
        "DOLLARDE(1.02,TRUE)",
        "DOLLARFR(1.125,FALSE)",
        "DOLLARDE(1.02,A1)",
        "DOLLARFR(1.125,\"16d\")",
        "DOLLARDE(\"0x1p0\",16)",
        "DOLLARDE(1.02,\"1E400\")",
        "NPV(0,\"2001-01-01\")",
        "NPV(0,\"16d\")",
    ] {
        assert_eq!(
            match modern_formula(
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
            CellValue::Error(CellError::Value),
            "{formula}"
        );
    }
    number(&workbook, "DOLLARDE(1.02,\" 16 \" )", 1.125);
    number(&workbook, "NPV(0,A1,TRUE,\"2\",)", 3.);
    number(&workbook, "IFERROR(NPV(0,\"2001-01-01\"),99)", 99.);
}

#[test]
fn irr_bound_admits_long_flow_and_preserves_fallback_and_no_root() {
    let mut workbook = workbook();
    for row in 0..100 {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(
                CellAddress { row, col: 0 },
                CellValue::Number(match row {
                    0 => -100.,
                    1 => 110.,
                    _ => 0.,
                }),
            )
            .unwrap();
    }
    let plan = FormulaPlan::bind(&poirot_parsing::formula::parse_formula("IRR(A1:A100)").unwrap())
        .unwrap();
    let outcome = run_plan(
        &workbook,
        0,
        AT,
        &plan,
        EvaluationInputs::default(),
        EvaluationBudgets {
            run_work: 1_000_000,
            ..EvaluationBudgets::default()
        },
    );
    assert!(
        matches!(outcome, EvaluationOutcome::Complete { value: CellValue::Number(value), .. } if (value - 0.1).abs() < 1e-10),
        "{outcome:?}"
    );
    // This root forces the source's safeguarded fallback after Newton overshoot.
    number(&workbook, "IRR({-100,1})", -0.99);
    assert_eq!(
        match modern_formula(
            &workbook,
            0,
            AT,
            "IRR({100,200})",
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Num)
    );
}

/// Execute financial controls under the current documented builtin contract.
fn modern_formula(
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
            .with_builtin_policy(poirot_eval::BuiltinSemanticsPolicy::ExcelDocumentation)
            .with_array_result_policy(poirot_eval::ArrayResultPolicy::ScalarIntersection),
        budgets,
    )
}
