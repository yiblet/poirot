//! Current text contracts from Microsoft's function references.
//! https://support.microsoft.com/en-us/excel/functions/proper-function
//! https://support.microsoft.com/en-us/excel/functions/numbervalue-function
//! https://support.microsoft.com/en-us/excel/functions/roman-function
use poirot::eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome, run_formula};
use poirot::model::{CellAddress, CellError, CellValue, Workbook};
const AT: CellAddress = CellAddress { row: 1, col: 3 };
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w
}

#[test]
fn modern_text_coercion_and_error_order() {
    let w = workbook();
    for (formula, expected) in [
        (
            r#"PROPER("hELLO wORLD")"#,
            CellValue::Text("Hello World".into()),
        ),
        ("PROPER(TRUE)", CellValue::Text("True".into())),
        ("PROPER(2.9999999999999996)", CellValue::Text("3".into())),
        // CODE exposes the documented numeric platform code.
        (r#"CODE("A")"#, CellValue::Number(65.)),
        // ROMAN defaults its optional form to classic notation.
        ("ROMAN(14)", CellValue::Text("XIV".into())),
        ("ROMAN(14,0)", CellValue::Text("XIV".into())),
        ("ROMAN(499,4)", CellValue::Text("ID".into())),
        ("ROMAN(-0.1,0)", CellValue::Error(CellError::Value)),
        ("ROMAN(1,1/0)", CellValue::Error(CellError::Div0)),
        (
            "ROMAN(0,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Name),
        ),
        ("NUMBERVALUE()", CellValue::Error(CellError::Value)),
        (
            "NUMBERVALUE(UNKNOWN_FUNCTION(),2,3,4)",
            CellValue::Error(CellError::Value),
        ),
        (
            r#"NUMBERVALUE("1.234,5",",",".")"#,
            CellValue::Number(1234.5),
        ),
        (
            r#"TEXTJOIN(",","TrUe",TRUE,2.9999999999999996)"#,
            CellValue::Text("TRUE,3".into()),
        ),
        (
            r#"TEXTJOIN(1/0,"invalid",UNKNOWN_FUNCTION())"#,
            CellValue::Error(CellError::Div0),
        ),
        (
            r#"TEXTJOIN(",","invalid",1/0)"#,
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(
            match poirot::eval::run_formula_with_configuration(
                &w,
                0,
                AT,
                formula,
                &poirot::eval::RunConfiguration::default()
                    .with_builtin_policy(poirot::eval::BuiltinSemanticsPolicy::ExcelDocumentation)
                    .with_text_policy(poirot::eval::TextSemanticsPolicy {
                        source: poirot::eval::TextSourceVersion::ExcelCompatibilityV2Documented
                    }),
                Default::default()
            ) {
                poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            expected,
            "{formula}"
        );
    }
}

#[test]
fn row_major_collection_stops_at_the_first_spreadsheet_error() {
    let mut w = workbook();
    let s = w.sheet_mut(0).unwrap();
    s.set_formula_with_cached_value(CellAddress { row: 0, col: 0 }, "1/0", CellValue::Number(0.))
        .unwrap();
    s.set_formula_with_cached_value(
        CellAddress { row: 0, col: 1 },
        "UNKNOWN_FUNCTION()",
        CellValue::Number(0.),
    )
    .unwrap();
    assert_eq!(
        match poirot::eval::run_formula(
            &w,
            0,
            AT,
            r#"TEXTJOIN(",",TRUE,A1:B1)"#,
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Div0)
    );
}

#[test]
fn text_budget_and_capability_failures_survive_iferror() {
    let w = workbook();
    for formula in [
        r#"IFERROR(PROPER("abcdef"),0)"#,
        r#"IFERROR(TEXTJOIN(",",TRUE,{"abc","def","ghi"}),0)"#,
        "IFERROR(ROMAN(3999,0),0)",
    ] {
        assert!(
            matches!(
                run_formula(
                    &w,
                    0,
                    AT,
                    formula,
                    EvaluationInputs::default(),
                    EvaluationBudgets {
                        run_work: 5,
                        ..EvaluationBudgets::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
    assert!(matches!(
        run_formula(
            &w,
            0,
            AT,
            r#"IFERROR(ROMAN(1,"uninstalled date policy"),0)"#,
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(0.),
            ..
        }
    ));
    // Invalid numeric form text produces a catchable VALUE error.
    assert_eq!(
        match poirot::eval::run_formula(
            &w,
            0,
            AT,
            r#"ROMAN(1,"uninstalled date policy")"#,
            Default::default(),
            Default::default()
        ) {
            poirot::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Value)
    );
    assert!(
        matches!(run_formula(&w, 0, AT, r#"IFERROR(TEXTJOIN(",",TRUE,),0)"#, Default::default(), Default::default()), EvaluationOutcome::Complete { value, .. } if value.text_ref().is_some_and(|text| text.len_units() == 0))
    );
}

#[test]
fn unknown_name_remains_a_catchable_spreadsheet_error() {
    let w = workbook();
    assert!(matches!(
        run_formula(
            &w,
            0,
            AT,
            "IFERROR(ROMAN(1,UNKNOWN_FUNCTION()),0)",
            EvaluationInputs::default(),
            EvaluationBudgets::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Number(0.0),
            ..
        }
    ));
}

#[test]
fn numbervalue_validates_explicit_separator_and_result_domains() {
    use poirot::eval::{BuiltinSemanticsPolicy, RunConfiguration, run_formula_with_configuration};
    let w = workbook();
    let excel =
        RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
    let value = |source: &str, config: &RunConfiguration| match run_formula_with_configuration(
        &w,
        0,
        AT,
        source,
        config,
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{source}: {outcome:?}"),
    };
    for source in [
        r#"NUMBERVALUE("123.45",".",".")"#,
        r#"NUMBERVALUE("123",".",".")"#,
        r#"NUMBERVALUE("1", "", ",")"#,
        r#"NUMBERVALUE("1,5", ".", "")"#,
        r#"NUMBERVALUE("12..")"#,
        r#"NUMBERVALUE("0x1p2")"#,
        r#"NUMBERVALUE("7D")"#,
        r#"NUMBERVALUE("1e+")"#,
        r#"NUMBERVALUE("Infinity")"#,
        r#"NUMBERVALUE("1e309%")"#,
        r#"NUMBERVALUE("1e-999")"#,
    ] {
        assert_eq!(
            value(source, &excel),
            CellValue::Error(CellError::Value),
            "{source}"
        );
    }
    for (source, expected) in [
        (r#"NUMBERVALUE("1.5", ".rest", ",rest")"#, 1.5),
        (r#"NUMBERVALUE("1⛄234§5", "§rest", "⛄rest")"#, 1234.5),
        (r#"NUMBERVALUE("1.5")"#, 1.5),
        (r#"NUMBERVALUE("1.5", ".", "")"#, 1.5),
        (r#"NUMBERVALUE("1.5",,",")"#, 1.5),
        (r#"NUMBERVALUE("3.5%")"#, 0.035),
        (r#"NUMBERVALUE("1e308%")"#, 1e306),
        (r#"NUMBERVALUE("0e999")"#, 0.),
        (r#"NUMBERVALUE("1e-320%%")"#, 0.),
        (r#"NUMBERVALUE("")"#, 0.),
    ] {
        assert_eq!(
            value(source, &excel),
            CellValue::Number(expected),
            "{source}"
        );
    }
    assert_eq!(
        value(r#"NUMBERVALUE("1^5","^",",")"#, &excel),
        CellValue::Number(1.5)
    );
    {
        let config = &excel;
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                AT,
                r#"IFERROR(NUMBERVALUE("1💧5","💧",","),0)"#,
                config,
                EvaluationBudgets::default(),
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
    let mut malformed = workbook();
    malformed
        .sheet_mut(0)
        .unwrap()
        .set_value(
            CellAddress { row: 0, col: 0 },
            poirot::model::ExcelText::from_units(vec![0xD800]),
        )
        .unwrap();
    assert!(matches!(
        run_formula_with_configuration(
            &malformed,
            0,
            AT,
            r#"IFERROR(NUMBERVALUE("1",A1,","),0)"#,
            &excel,
            EvaluationBudgets::default(),
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    let comma = excel.with_number_locale(
        poirot_parsing::number::NumberLocale::new(",", Some("."), None, "%", ":").unwrap(),
    );
    assert_eq!(
        value(r#"NUMBERVALUE("1.234,5")"#, &comma),
        CellValue::Number(1234.5)
    );
    assert_eq!(
        value(r#"NUMBERVALUE("1.5",".",",")"#, &comma),
        CellValue::Number(1.5)
    );
    assert_eq!(
        value(r#"NUMBERVALUE("1.234,5",, ".")"#, &comma),
        CellValue::Number(1234.5)
    );
    for source in [
        r#"IFERROR(NUMBERVALUE("12345"),0)"#,
        r#"IFERROR(NUMBERVALUE("1",""),0)"#,
    ] {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &w,
                    0,
                    AT,
                    source,
                    &comma,
                    EvaluationBudgets {
                        run_work: 5,
                        ..Default::default()
                    },
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{source}"
        );
    }
    assert_eq!(
        value(r#"NUMBERVALUE(1/0,".",",")"#, &comma),
        CellValue::Error(CellError::Div0)
    );
}
