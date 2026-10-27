//! Shared Excel criterion admission; pinned POI predicates retain their source behavior.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
use poirot::model::{CellError, CellValue, Workbook};

fn target(at: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: at.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("S").unwrap();
    for (at, value) in [
        ("A1", CellValue::Blank),
        ("A2", 0.0.into()),
        ("A3", "".into()),
        ("A4", 2.0.into()),
        ("B1", 10.0.into()),
        ("B2", 20.0.into()),
        ("B3", 30.0.into()),
        ("B4", 40.0.into()),
    ] {
        s.set_value(target(at).address, value).unwrap();
    }
    s.set_formula(target("D2").address, "\"\"").unwrap();
    w
}
fn excel() -> RunConfiguration {
    RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)
}
fn formula(name: &str, criterion: &str) -> String {
    if matches!(name, "SUMIF" | "AVERAGEIF") {
        return format!("{name}(A1:A4,{criterion},B1:B4)");
    }
    format!(
        "{name}({}A1:A4,{criterion})",
        if matches!(name, "COUNTIFS" | "COUNTIF") {
            ""
        } else {
            "B1:B4,"
        }
    )
}
fn value(w: &Workbook, f: &str, config: &RunConfiguration) -> CellValue {
    match run_formula_with_configuration(
        w,
        0,
        target("Z1").address,
        f,
        config,
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{f}: {other:?}"),
    }
}
#[test]
fn criteria_admission_distinguishes_empty_references_from_empty_text() {
    let w = book();
    for (name, zero, empty) in [
        ("COUNTIF", 1., 2.),
        ("SUMIF", 20., 40.),
        ("AVERAGEIF", 20., 20.),
        ("MAXIFS", 20., 30.),
        ("MINIFS", 20., 10.),
        ("SUMIFS", 20., 40.),
        ("COUNTIFS", 1., 2.),
        ("AVERAGEIFS", 20., 20.),
    ] {
        for criterion in [
            "D1",
            "INDEX(D1:D1,1)",
            "IF(TRUE,D1,1)",
            "IF({1},D1,0)",
            "0",
            "\"0\"",
            "\"=0\"",
        ] {
            assert_eq!(
                value(&w, &formula(name, criterion), &excel()),
                CellValue::Number(zero),
                "{name}, {criterion}"
            );
        }
        for criterion in ["\"\"", "D2"] {
            assert_eq!(
                value(&w, &formula(name, criterion), &excel()),
                CellValue::Number(empty)
            );
        }
    }
    assert_eq!(
        value(&w, "ISBLANK(IF({1},D1,0))", &excel()),
        CellValue::Boolean(true)
    );
}
#[test]
fn criterion_changes_invalidate_retained_results() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("Z1").address, formula("SUMIFS", "D1"))
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(EvaluationPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = EvaluationContext::default();
    // Blank criteria select numeric zero (B2=20); empty text selects A1/A3 (10+30).
    for (criterion, expected) in [
        (CellValue::Blank, 20.),
        ("".into(), 40.),
        (CellValue::Blank, 20.),
    ] {
        overlay.set_value(target("D1"), criterion).unwrap();
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target("Z1"))
                .unwrap(),
            CellValue::Number(expected)
        );
    }
}

#[test]
fn omitted_criteria_and_operational_failures_escape_iferror() {
    let w = book();
    for name in ["MAXIFS", "MINIFS", "SUMIFS", "COUNTIFS", "AVERAGEIFS"] {
        for criterion in ["", "IFS(TRUE,)"] {
            let f = format!("IFERROR({},99)", formula(name, criterion));
            assert!(
                matches!(
                    run_formula_with_configuration(
                        &w,
                        0,
                        target("Z1").address,
                        &f,
                        &excel(),
                        EvaluationBudgets::default()
                    ),
                    EvaluationOutcome::Aborted { .. }
                ),
                "{f}"
            );
        }
        let f = format!("IFERROR({},99)", formula(name, "D1"));
        for budgets in [
            EvaluationBudgets {
                materialized_values: 1,
                ..Default::default()
            },
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            },
        ] {
            assert!(matches!(
                run_formula_with_configuration(&w, 0, target("Z1").address, &f, &excel(), budgets),
                EvaluationOutcome::Aborted { .. }
            ));
        }
        let f = format!("IFERROR({},99)", formula(name, "\"long criterion\""));
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                target("Z1").address,
                &f,
                &excel(),
                EvaluationBudgets {
                    text_bytes: 1,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}

#[test]
fn contextual_thresholds_keep_literal_wildcard_boolean_and_error_criteria_typed() {
    let mut w = Workbook::new();
    let sheet = w.add_sheet("S").unwrap();
    for (at, value) in [
        ("A1", CellValue::Number(0.04)),
        ("A2", CellValue::Text("4%".into())),
        ("A3", CellValue::Text("3%%".into())),
        ("A4", CellValue::Text("$3x".into())),
        ("A5", CellValue::Text("1 1/0".into())),
        ("A6", CellValue::Text("32/32/2020".into())),
        ("A7", CellValue::Text("3%abc".into())),
        ("A8", CellValue::Text("3%*".into())),
        ("A9", CellValue::Boolean(true)),
        ("A10", CellValue::Error(poirot::model::CellError::NA)),
    ] {
        sheet.set_value(target(at).address, value).unwrap();
    }
    // These neighboring expectations were authored before either engine ran and
    // independently corroborated in isolated Calc snapshots.
    for (f, expected) in [
        (r#"COUNTIF(A1:A2,">3%")"#, 1.),
        (r#"COUNTIF(A2,"4%")"#, 1.),
        (r#"COUNTIF(A2,"<>4%")"#, 0.),
        (r#"COUNTIF(A3,"3%%")"#, 1.),
        (r#"COUNTIF(A3,">3%%")"#, 0.),
        (r#"COUNTIF(A4,"$3x")"#, 1.),
        (r#"COUNTIF(A5,"1 1/0")"#, 1.),
        (r#"COUNTIF(A6,"32/32/2020")"#, 1.),
        (r#"COUNTIF(A7:A8,"3%*")"#, 2.),
        (r#"COUNTIF(A8,"3%~*")"#, 1.),
        (r#"COUNTIF(A9,"TRUE")"#, 1.),
        (r##"COUNTIF(A10,"#N/A")"##, 1.),
    ] {
        assert_eq!(value(&w, f, &excel()), CellValue::Number(expected), "{f}");
    }
    let localized = excel().with_number_locale(
        poirot_parsing::number::NumberLocale::new(",", Some("."), Some("€"), "%", ":").unwrap(),
    );
    assert_eq!(value(&w, r#"COUNTIF(A1,">€0,03")"#, &localized), 1.0.into());
    for budgets in [
        EvaluationBudgets {
            run_work: 20,
            ..Default::default()
        },
        EvaluationBudgets {
            text_bytes: 2,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                target("Z1").address,
                r#"IFERROR(COUNTIF(A1,">$0.03"),99)"#,
                &excel(),
                budgets,
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}

#[test]
fn contextual_criteria_invalidate_retained_locale_and_date_order_reads() {
    let mut w = Workbook::new();
    let sheet = w.add_sheet("S").unwrap();
    sheet.set_value(target("A1").address, 10.).unwrap();
    sheet.set_value(target("A2").address, 43840.).unwrap();
    sheet
        .set_formula(target("B1").address, r#"COUNTIF(A1,">1,234")"#)
        .unwrap();
    sheet
        .set_formula(target("B2").address, r#"COUNTIF(A2,">1/2/2020")"#)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(EvaluationPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .build()
        .unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        0.0.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        1.0.into()
    );
    context.number_locale =
        poirot_parsing::number::NumberLocale::new(",", Some("."), Some("€"), "%", ":").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B1"))
            .unwrap(),
        1.0.into()
    );
    context.localization.date_order = DateOrder::DayMonthYear;
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        0.0.into()
    );
    context.localization.date_order = DateOrder::MonthDayYear;
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        1.0.into()
    );
}

#[test]
fn criterion_calendar_threshold_uses_workbook_epoch() {
    let mut w = Workbook::new();
    w.add_sheet("S")
        .unwrap()
        .set_value(target("A1").address, 1.)
        .unwrap();
    let f = r#"COUNTIF(A1,">1/1/1904")"#;
    assert_eq!(value(&w, f, &excel()), 0.0.into());
    w.date_system = poirot::model::DateSystem::Excel1904;
    assert_eq!(value(&w, f, &excel()), 1.0.into());
}

#[test]
fn selected_result_errors_propagate_after_row_selection() {
    let mut w = book();
    for (at, v) in [
        ("A1", 1.0.into()),
        ("A2", 2.0.into()),
        ("A3", 2.0.into()),
        ("B1", CellValue::Error(CellError::Div0)),
        ("B2", 20.0.into()),
        ("B3", CellValue::Error(CellError::NA)),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(target(at).address, v)
            .unwrap();
    }
    {
        let config = excel();
        for name in [
            "SUMIF",
            "AVERAGEIF",
            "SUMIFS",
            "AVERAGEIFS",
            "MAXIFS",
            "MINIFS",
        ] {
            let f = if matches!(name, "SUMIF" | "AVERAGEIF") {
                format!("{name}(A1:A3,2,B1:B3)")
            } else {
                format!("{name}(B1:B3,A1:A3,2)")
            };
            let expected = CellValue::Error(CellError::NA);
            assert_eq!(value(&w, &f, &config), expected, "{f}");
        }
        assert_eq!(
            value(&w, "AVERAGEIF(A1:A3,99,B1:B3)", &config),
            CellValue::Error(CellError::Div0)
        );
        assert_eq!(value(&w, "AVERAGEIF(A2:A2,2,B2:B2)", &config), 20.0.into());
        assert_eq!(
            value(&w, "AVERAGEIFS(B1:B3,A1:A3,2,A1:A3,99)", &config),
            CellValue::Error(CellError::Div0)
        );
        assert_eq!(value(&w, "COUNTIFS(A1:A3,2)", &config), 2.0.into());
    }
}

#[test]
fn averageif_skips_rejected_payloads_and_preserves_selected_host_failures() {
    let mut w = book();
    // An unsupported selected payload must abort; rejected payloads stay unread.
    w.sheet_mut(0)
        .unwrap()
        .set_formula(target("B1").address, "COUNTIF(A1:A4,)")
        .unwrap();
    {
        let config = excel();
        assert_eq!(value(&w, "SUMIF(A1:A4,2,B1:B4)", &config), 40.0.into());
        assert_eq!(value(&w, "AVERAGEIF(A1:A4,2,B1:B4)", &config), 40.0.into());
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                target("Z1").address,
                r#"IFERROR(AVERAGEIF(A1:A4,"",B1:B4),99)"#,
                &config,
                EvaluationBudgets::default()
            ),
            EvaluationOutcome::Aborted { .. }
        ));
        assert!(matches!(
            run_formula_with_configuration(
                &w,
                0,
                target("Z1").address,
                "IFERROR(AVERAGEIF(A1:A4,2,B1:B4),99)",
                &config,
                EvaluationBudgets {
                    run_work: 1,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}

#[test]
fn conditional_reducers_read_payloads_only_after_selection() {
    // Microsoft AVERAGEIF specifies the arithmetic mean of matching cells and
    // permits a smaller average_range anchored at its top-left cell:
    // https://support.microsoft.com/en-us/excel/functions/averageif-function
    // Expected mean is (20 + 30) / 2 = 25, independently of the evaluator.
    let mut w = book();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value(target("A1").address, 0.).unwrap();
    sheet.set_value(target("A2").address, 1.).unwrap();
    sheet.set_value(target("A3").address, 1.).unwrap();
    sheet.set_formula(target("B1").address, "B1").unwrap();
    for formula in ["AVERAGEIF(A1:A3,1,B1)", "AVERAGEIFS(B1:B3,A1:A3,1)"] {
        assert_eq!(value(&w, formula, &excel()), 25.0.into());
    }
    for formula in ["SUMIF(A1:A3,1,B1)", "SUMIFS(B1:B3,A1:A3,1)"] {
        assert_eq!(value(&w, formula, &excel()), 50.0.into());
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value(target("B1").address, CellValue::Error(CellError::NA))
        .unwrap();
    assert_eq!(
        value(&w, "AVERAGEIF(A1:A3,0,B1)", &excel()),
        CellValue::Error(CellError::NA)
    );
}

#[test]
fn criteria_array_results_survive_child_first_compilation() {
    // Criteria select the numbers 0 and 2 from A2/A4; the corresponding payloads
    // are 20 and 40. Collection ranges must not determine the output footprint.
    let w = book();
    for (formula, expected) in [
        ("SUM(COUNTIF(A1:A4,{0,2;2,0}))", 4.),
        ("SUM(COUNTIFS(A1:A4,{0,2}))", 2.),
        ("SUM(SUMIF(A1:A4,{0,2},B1:B4))", 60.),
        ("SUM(AVERAGEIF(A1:A4,{0,2},B1:B4))", 60.),
        ("SUM(SUMIFS(B1:B4,A1:A4,{0,2}))", 60.),
        ("SUM(AVERAGEIFS(B1:B4,A1:A4,{0,2}))", 60.),
        ("SUM(MAXIFS(B1:B4,A1:A4,{0,2}))", 60.),
        ("SUM(MINIFS(B1:B4,A1:A4,{0,2}))", 60.),
    ] {
        assert_eq!(value(&w, formula, &excel()), expected.into(), "{formula}");
    }
}

#[test]
fn reference_criterion_controls_spill_when_children_are_requested_first() {
    // Reference controls participate in the ordinary dynamic-array broadcasting:
    // https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
    // Criteria 0 and 2 each match once, selecting payloads 20 and 40 respectively.
    let mut w = book();
    let sheet = w.sheet_mut(0).unwrap();
    sheet.set_value(target("D1").address, 0.).unwrap();
    sheet.set_value(target("D2").address, 2.).unwrap();
    sheet
        .set_formula(target("F1").address, "COUNTIF(A1:A4,D1:D2)")
        .unwrap();
    sheet
        .set_formula(target("G1").address, "SUMIFS(B1:B4,A1:A4,D1:D2)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&w)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(target("F2")).unwrap(), 1.0.into());
    assert_eq!(direct.cell(target("G2")).unwrap(), 40.0.into());
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("G2"))
            .unwrap(),
        40.0.into()
    );
    overlay.set_value(target("D2"), 99.).unwrap();
    for cell in ["F2", "G2"] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            0.0.into()
        );
    }
}
