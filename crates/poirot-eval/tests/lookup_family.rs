//! Formula-level coverage of source-sensitive deferred lookup execution.
use poirot_eval::{
    EvaluationBudgets, EvaluationInputs, EvaluationOutcome, bound::FormulaPlan, run_plan,
};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook.add_sheet("Other").unwrap();
    for (address, value) in [("A1", 1.), ("A2", 2.), ("B1", 10.), ("B2", 20.)] {
        workbook
            .sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    workbook
}
#[test]
fn reference_identity_and_deferred_operands_follow_pinned_source() {
    let mut workbook = workbook();
    workbook.define_name("OneCell", "A1", None).unwrap();
    workbook.define_name("OneArea", "A1:A1", None).unwrap();
    let at = CellAddress { row: 0, col: 3 };
    for (formula, expected) in [
        (
            r#"ADDRESS(1,2,1,UNKNOWN_FUNCTION(),"Other")"#,
            CellValue::Text("Other!$B$1".into()),
        ),
        ("AREAS((A1,B2),UNKNOWN_FUNCTION())", CellValue::Number(2.)),
        ("HYPERLINK(UNKNOWN_FUNCTION(),42)", CellValue::Number(42.)),
        ("SHEET(A1,UNKNOWN_FUNCTION())", CellValue::Number(1.)),
        ("SINGLE(OneCell)", CellValue::Error(CellError::Value)),
        ("SINGLE(OneArea)", CellValue::Number(1.)),
        (
            "SINGLE(HYPERLINK(0,A1))",
            CellValue::Error(CellError::Value),
        ),
        ("SINGLE(HYPERLINK(0,A1:A1))", CellValue::Number(1.)),
        (
            r#"SINGLE(INDIRECT("A1"))"#,
            CellValue::Error(CellError::Value),
        ),
        (r#"SINGLE(INDIRECT("A1:A1"))"#, CellValue::Number(1.)),
        (r#"INDIRECT("A2")"#, CellValue::Number(2.)),
        (r#"INDIRECT("R[1]C[-3]",FALSE)"#, CellValue::Number(2.)),
        ("SUM(OFFSET(A1,0,1,2,1))", CellValue::Number(30.)),
        ("XMATCH(2,A1:A2)", CellValue::Number(2.)),
        ("XLOOKUP(2,A1:A2,B1:B2)", CellValue::Number(20.)),
        (
            "XLOOKUP(2,A1:A2,B1:B2,1/0)",
            CellValue::Error(CellError::Div0),
        ),
        (
            r#"INDIRECT("NoSuchSheet!A1")"#,
            CellValue::Error(CellError::Ref),
        ),
    ] {
        let plan =
            FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
        assert_eq!(
            match poirot_eval::run_plan(
                &workbook,
                0,
                at,
                &plan,
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
fn unsupported_and_budget_failures_survive_iferror() {
    let workbook = workbook();
    let at = CellAddress { row: 0, col: 3 };
    let supported_short = FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula("SUM(IFERROR(OFFSET(A1,40000,0),99))").unwrap(),
    )
    .unwrap();
    assert_eq!(
        match poirot_eval::run_plan(
            &workbook,
            0,
            at,
            &supported_short,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Number(0.)
    );
    for formula in [
        r#"IFERROR(INDIRECT("[missing.xlsx]Sheet1!A1"),99)"#,
        r#"IFERROR(XMATCH("é",{"é"}),99)"#,
        "IFERROR(LOOKUP(1,{1,2;3,4},{1,2}),99)",
    ] {
        let plan =
            FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
        assert!(
            matches!(
                run_plan(
                    &workbook,
                    0,
                    at,
                    &plan,
                    EvaluationInputs::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    let plan = FormulaPlan::bind(
        &poirot_parsing::formula::parse_formula(r#"IFERROR(XMATCH("a*",{"abc","def"},2),99)"#)
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        run_plan(
            &workbook,
            0,
            at,
            &plan,
            EvaluationInputs::default(),
            EvaluationBudgets {
                materialized_values: 2,
                ..EvaluationBudgets::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
