use poirot_eval::{EvaluationBudgets, EvaluationInputs, EvaluationOutcome, run_formula};
use poirot_model::{CellAddress, CellError, CellValue, Workbook};
const AT: CellAddress = CellAddress { row: 1, col: 3 };
fn workbook() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("Sheet1").unwrap();
    w
}

#[test]
fn text_source_coercion_and_lazy_error_order() {
    let w = workbook();
    for (formula, expected) in [
        (
            r#"PROPER("hELLO wORLD")"#,
            CellValue::Text("Hello World".into()),
        ),
        (
            r#"PROPER("ßABC 𐐨abc")"#,
            CellValue::Text("SSabc 𐐨Abc".into()),
        ),
        ("PROPER(TRUE)", CellValue::Text("True".into())),
        ("PROPER(2.9999999999999996)", CellValue::Text("3".into())),
        // The default POI text policy returns CODE as text.
        (r#"CODE("A")"#, CellValue::Text("65".into())),
        // ROMAN requires its form argument under this source policy.
        ("ROMAN(14)", CellValue::Error(CellError::Value)),
        ("ROMAN(14,0)", CellValue::Text("XIV".into())),
        ("ROMAN(499,4)", CellValue::Text("ID".into())),
        ("ROMAN(-0.1,0)", CellValue::Error(CellError::Value)),
        ("ROMAN(1,1/0)", CellValue::Error(CellError::Num)),
        (
            "ROMAN(0,UNKNOWN_FUNCTION())",
            CellValue::Text(String::new()),
        ),
        ("NUMBERVALUE()", CellValue::Number(0.)),
        (
            "NUMBERVALUE(UNKNOWN_FUNCTION(),2,3,4)",
            CellValue::Number(0.),
        ),
        (
            r#"NUMBERVALUE("1.234,5",",",".")"#,
            CellValue::Number(1234.5),
        ),
        (
            r#"TEXTJOIN({"x","y";"-","/"},TRUE,{"a","";"b","c"},"d")"#,
            CellValue::Text("a-b/c-d".into()),
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
            match poirot_eval::run_formula_with_configuration(
                &w,
                0,
                AT,
                formula,
                &poirot_eval::RunConfiguration::default()
                    .with_builtin_policy(poirot_eval::BuiltinSemanticsPolicy::PinnedPoi)
                    .with_text_policy(poirot_eval::TextSemanticsPolicy {
                        source: poirot_eval::TextSourceVersion::Poi7321209OpenJdk21Unicode15
                    }),
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
fn reference_delimiters_use_only_last_row() {
    let mut w = workbook();
    let s = w.sheet_mut(0).unwrap();
    s.set_formula_with_cached_value(
        CellAddress { row: 0, col: 0 },
        "UNKNOWN_FUNCTION()",
        CellValue::Number(0.),
    )
    .unwrap();
    s.set_value(CellAddress { row: 1, col: 0 }, CellValue::Text("-".into()))
        .unwrap();
    s.set_value(CellAddress { row: 1, col: 1 }, CellValue::Text("/".into()))
        .unwrap();
    assert_eq!(
        match poirot_eval::run_formula(
            &w,
            0,
            AT,
            r#"TEXTJOIN(A1:B2,FALSE,"a","b","c")"#,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Text("a-b/c".into())
    );
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
        match poirot_eval::run_formula(
            &w,
            0,
            AT,
            r#"TEXTJOIN(",",TRUE,A1:B1)"#,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
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
    // ROMAN maps form conversion failure to NUM; IFERROR still catches it.
    assert_eq!(
        match poirot_eval::run_formula(
            &w,
            0,
            AT,
            r#"ROMAN(1,"uninstalled date policy")"#,
            Default::default(),
            Default::default()
        ) {
            poirot_eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        CellValue::Error(CellError::Num)
    );
    {
        let formula = r#"IFERROR(TEXTJOIN(",",TRUE,),0)"#;
        assert!(
            matches!(
                run_formula(
                    &w,
                    0,
                    AT,
                    formula,
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
    use poirot_eval::{BuiltinSemanticsPolicy, RunConfiguration, run_formula_with_configuration};
    let w = workbook();
    let pinned = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    let excel = pinned
        .clone()
        .with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation);
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
    for (source, expected) in [
        (r#"NUMBERVALUE("12..")"#, 12.),
        (r#"NUMBERVALUE("0x1p2")"#, 4.),
        (r#"NUMBERVALUE("7D")"#, 7.),
    ] {
        assert_eq!(value(source, &pinned), CellValue::Number(expected));
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
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            AT,
            r#"IFERROR(NUMBERVALUE("1^5","^",","),0)"#,
            &pinned,
            EvaluationBudgets::default(),
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    let mut malformed = workbook();
    malformed
        .sheet_mut(0)
        .unwrap()
        .set_value(
            CellAddress { row: 0, col: 0 },
            poirot_model::ExcelText::from_units(vec![0xD800]),
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
        poirot_parsing::number::NumberLocaleV2::new(",", Some("."), None, "%", ":").unwrap(),
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
