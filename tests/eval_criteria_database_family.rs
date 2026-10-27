//! Actual formulas for typed criteria/database bridges; pinned POI source contracts.
use poirot::eval::{
    EvaluationBudgets, EvaluationInputs, EvaluationOutcome, compile::bound::FormulaPlan, run_plan,
};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    let s = w.sheet_mut(0).unwrap();
    for (address, value) in [
        ("A1", 1.),
        ("A2", 2.),
        ("A3", 2.),
        ("B1", 10.),
        ("B2", 20.),
        ("B3", 30.),
    ] {
        s.set_value(address.parse().unwrap(), value).unwrap();
    }
    for (address, value) in [
        ("D1", CellValue::Text("N".into())),
        ("E1", CellValue::Text("Tag".into())),
        ("D2", 2.0.into()),
        ("D3", 4.0.into()),
        ("D4", 6.0.into()),
        ("E2", "A".into()),
        ("E3", "B".into()),
        ("E4", "A".into()),
        ("G1", "Tag".into()),
        ("G2", "A".into()),
        ("H1", "N".into()),
        ("H2", CellValue::Blank),
    ] {
        s.set_value(address.parse().unwrap(), value).unwrap();
    }
    w
}
fn plan(formula: &str) -> FormulaPlan {
    FormulaPlan::bind(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap()
}
fn check(w: &Workbook, formula: &str, expected: CellValue) {
    assert_eq!(
        match poirot::eval::run_plan(
            w,
            0,
            CellAddress { row: 0, col: 10 },
            &plan(formula),
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        expected,
        "{formula}"
    );
}
#[test]
fn criteria_functions_execute_and_preserve_source_coercion() {
    let w = workbook();
    for (formula, n) in [
        ("COUNTIF(A1:A3,2)", 2.),
        ("COUNTIFS(A1:A3,2,B1:B3,\">20\")", 1.),
        ("SUMIF(A1:A3,2,B1)", 50.),
        ("SUMIFS(B1:B3,A1:A3,2)", 50.),
        ("AVERAGEIF(A1:A3,2,B1)", 25.),
        ("AVERAGEIFS(B1:B3,A1:A3,2)", 25.),
        ("MAXIFS(B1:B3,A1:A3,2)", 30.),
        ("MINIFS(B1:B3,A1:A3,2)", 20.),
        ("SUM(COUNTIF(A1:A3,{1,2}))", 3.),
        ("COUNTIF({\"2\",TRUE,2},2)", 2.),
        ("COUNTIF({\"Ab\",\"Ac\",\"A*\"},\"A~*\")", 1.),
        ("SUBTOTAL(9,1,TRUE,\"2\")", 4.),
        ("SUBTOTAL(109,{1,2,TRUE})", 3.),
        ("COUNTIF({1,#N/A},#N/A)", 1.),
    ] {
        check(&w, formula, n.into());
    }
    for (formula, e) in [
        ("COUNTIF(1,1)", CellError::Value),
        ("SUMIFS(B1:B3,A1:A2,1/0)", CellError::Value),
        ("AVERAGEIFS(B1:B3,A1:A3,1/0)", CellError::Div0),
        ("AVERAGEIF(A1:A3,99)", CellError::Div0),
        ("SUBTOTAL(12,1/0)", CellError::Value),
    ] {
        check(&w, formula, CellValue::Error(e));
    }
}
#[test]
fn every_database_aggregation_runs_on_matched_rows() {
    let w = workbook();
    for (name, n) in [
        ("DAVERAGE", 4.),
        ("DCOUNT", 2.),
        ("DCOUNTA", 2.),
        ("DMAX", 6.),
        ("DMIN", 2.),
        ("DPRODUCT", 12.),
        ("DSUM", 8.),
        ("DVAR", 8.),
        ("DVARP", 4.),
        ("DSTDEV", 8f64.sqrt()),
        ("DSTDEVP", 2.),
    ] {
        check(&w, &format!("{name}(D1:E4,\"N\",G1:G2)"), n.into());
    }
    check(&w, "DGET(D1:E4,\"N\",{\"N\";4})", 4.0.into());
    check(
        &w,
        "DGET(D1:E4,\"N\",G1:G2)",
        CellValue::Error(CellError::Num),
    );
    check(
        &w,
        "DGET(D1:E4,\"N\",{\"N\";99})",
        CellValue::Error(CellError::Value),
    );
    check(&w, "DCOUNT(D1:E4,,G1:G2)", 2.0.into());
    check(&w, "DSUM(D1:E4,1.5,H1:H2)", 0.0.into());
    check(&w, "DSUM(1,1,1/0)", CellValue::Error(CellError::Value));
    check(
        &w,
        "DSUM(D1:E4,1/0,H1:H2)",
        CellValue::Error(CellError::Div0),
    );
    check(
        &w,
        r#"DSUM(D1:E4,1,{"Tag","Unknown";"Z",TRUE})"#,
        0.0.into(),
    );
    check(
        &w,
        r#"DSUM(D1:E4,1,{"N";">bad"})"#,
        CellValue::Error(CellError::Value),
    );
    check(&w, "DCOUNT(D1:E4,1/0,G1:G2)", 2.0.into());
}
#[test]
fn rejected_rows_and_later_criteria_ranges_stay_unread() {
    let mut w = workbook();
    w.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value("B1".parse().unwrap(), "SUBTOTAL(109,A1)", 0.0.into())
        .unwrap();
    check(&w, "SUMIF(A1:A3,2,B1:B3)", 50.0.into());
    check(&w, "COUNTIFS(A1:A3,99,B1:B3,1)", 0.0.into());
    w.sheet_mut(0)
        .unwrap()
        .set_formula_with_cached_value("D3".parse().unwrap(), "SUBTOTAL(109,A1)", 0.0.into())
        .unwrap();
    check(&w, "DSUM(D1:E4,\"N\",G1:G2)", 8.0.into());
    check(&w, "COUNTIF(B1:B3,C1)", 0.0.into());
}
#[test]
fn formula_criteria_and_resource_failures_escape_iferror() {
    let w = workbook();
    check(&w, "IFERROR(SUBTOTAL(109,A1:A3),99)", 5.0.into());
    check(&w, "SUM(COUNTIF(A1:A3,B1:B3))", 0.0.into());
    let formula = "IFERROR(DSUM(D1:E4,1,{\"Unknown\";TRUE}),99)";
    assert!(
        matches!(
            poirot::eval::run_plan_with_configuration(
                &w,
                0,
                CellAddress { row: 0, col: 10 },
                &plan(formula),
                &poirot::eval::RunConfiguration::default(),
                EvaluationBudgets::default()
            ),
            EvaluationOutcome::Aborted { .. }
        ),
        "{formula}"
    );
    for formula in [
        "IFERROR(COUNTIF(A1:A3,\"*\"),99)",
        "IFERROR(DSUM(D1:E4,1,G1:G2),99)",
    ] {
        assert!(
            matches!(
                poirot::eval::run_plan_with_configuration(
                    &w,
                    0,
                    CellAddress { row: 0, col: 10 },
                    &plan(formula),
                    &poirot::eval::RunConfiguration::default(),
                    EvaluationBudgets {
                        run_work: 0,
                        ..EvaluationBudgets::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}

#[test]
fn subtotal_integer_coercion_preserves_source_error_class() {
    let w = workbook();
    for (formula, error_type) in [
        ("ERROR.TYPE(SUBTOTAL(2147483648,1))", 6.),
        ("ERROR.TYPE(SUBTOTAL(-2147483649,1))", 6.),
        ("ERROR.TYPE(SUBTOTAL(2147483647.9,1))", 3.),
        ("ERROR.TYPE(SUBTOTAL(-2147483648,1))", 3.),
        ("ERROR.TYPE(SUBTOTAL(C1,1))", 3.),
    ] {
        check(&w, formula, error_type.into());
    }
    check(&w, "SUBTOTAL(9.9,1)", 1.0.into());
}
#[test]
fn missing_codes_and_predicates_escape_iferror_but_blank_cells_remain_values() {
    let w = workbook();
    check(&w, "COUNTIF(A1:A3,C1)", 0.0.into());
    check(&w, "SUMIF(A1:A3,C1,B1:B3)", 0.0.into());
    for formula in [
        "IFERROR(SUBTOTAL(IFS(TRUE,),1),99)",
        "IFERROR(SUBTOTAL(,1),99)",
        "IFERROR(COUNTIF(A1:A1,IFS(TRUE,)),99)",
        "IFERROR(COUNTIF(A1:A1,),99)",
        "IFERROR(SUMIF(A1:A1,IFS(TRUE,),B1:B1),99)",
        "IFERROR(AVERAGEIF(A1:A1,IFS(TRUE,)),99)",
        "IFERROR(COUNTIFS(A1:A1,IFS(TRUE,)),99)",
        "IFERROR(SUMIFS(B1:B1,A1:A1,IFS(TRUE,)),99)",
        "IFERROR(AVERAGEIFS(B1:B1,A1:A1,IFS(TRUE,)),99)",
        "IFERROR(MAXIFS(B1:B1,A1:A1,IFS(TRUE,)),99)",
        "IFERROR(MINIFS(B1:B1,A1:A1,IFS(TRUE,)),99)",
    ] {
        assert!(
            matches!(
                run_plan(
                    &w,
                    0,
                    CellAddress { row: 0, col: 10 },
                    &plan(formula),
                    EvaluationInputs::default(),
                    EvaluationBudgets::default()
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}

#[test]
fn database_catches_missing_fields_and_accepts_observed_numeric_headers() {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    for (address, value) in [
        ("A1", "".into()),
        ("A2", 7.0.into()),
        ("C1", "".into()),
        ("C2", CellValue::Blank),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    check(
        &w,
        "DSUM(A1:A2,IFS(TRUE,),C1:C2)",
        CellValue::Error(CellError::Value),
    );
    check(&w, "ERROR.TYPE(DSUM(A1:A2,IFS(TRUE,),C1:C2))", 3.0.into());
    check(&w, "IFERROR(DSUM(A1:A2,IFS(TRUE,),C1:C2),99)", 99.0.into());
    check(&w, "DCOUNT(A1:A2,IFS(TRUE,),C1:C2)", 1.0.into());
    check(&w, "DCOUNTA(A1:A2,IFS(TRUE,),C1:C2)", 1.0.into());
    // Genuine empty-text header selection is different from MissingArgEval.
    check(&w, r#"DSUM(A1:A2,"",C1:C2)"#, 7.0.into());
    let positional_header_inputs: [(&str, CellValue); 6] = [
        ("A1", "X".into()),
        ("B1", "Y".into()),
        ("A2", 3.0.into()),
        ("B2", 7.0.into()),
        ("D1", 1.0.into()),
        ("D2", 3.0.into()),
    ];
    for (address, value) in positional_header_inputs {
        w.sheet_mut(0)
            .unwrap()
            .set_value(address.parse().unwrap(), value)
            .unwrap();
    }
    check(&w, "DSUM(A1:B2,2,D1:D2)", 7.0.into());
    // NumberEval implements StringValueEval, so DStarRunner accepts a numeric
    // positional heading. A missing argument has a different runtime type.
    // Blank conditions skip the header check in the source.
    w.sheet_mut(0)
        .unwrap()
        .set_value("D2".parse().unwrap(), CellValue::Blank)
        .unwrap();
    check(&w, "DSUM(A1:B2,2,D1:D2)", 7.0.into());
}
