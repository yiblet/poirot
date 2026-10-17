//! Shared Excel criterion admission; pinned POI predicates retain their source behavior.
use poirot_eval::{
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellValue, Workbook};

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
    s.set_formula(target("D2").address, "\"\"", CellValue::Blank)
        .unwrap();
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
        // Pinned POI does not compile BlankEval as a numeric predicate.
        let expected = if matches!(name, "AVERAGEIFS" | "AVERAGEIF") {
            CellValue::Error(poirot_model::CellError::Div0)
        } else {
            0.0.into()
        };
        assert_eq!(
            value(&w, &formula(name, "D1"), &RunConfiguration::default()),
            expected
        );
    }
    assert_eq!(
        value(&w, "ISBLANK(IF({1},D1,0))", &excel()),
        CellValue::Boolean(true)
    );
}
#[test]
fn policy_switches_invalidate_retained_criteria_results() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_formula(
            target("Z1").address,
            formula("SUMIFS", "D1"),
            CellValue::Blank,
        )
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        0.0.into()
    );
    overlay
        .set_policy(OverlayPolicy {
            builtin: BuiltinSemanticsPolicy::ExcelDocumentation,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        20.0.into()
    );
    overlay.set_value(target("D1"), "").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        40.0.into()
    );
    overlay.set_value(target("D1"), CellValue::Blank).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        20.0.into()
    );
    overlay.set_policy(OverlayPolicy::default()).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("Z1"))
            .unwrap(),
        0.0.into()
    );
}
#[test]
fn omitted_criteria_and_operational_failures_escape_iferror() {
    let w = book();
    for name in ["MAXIFS", "MINIFS", "SUMIFS", "COUNTIFS", "AVERAGEIFS"] {
        for criterion in ["", "IFS(TRUE,)", "D1:D2"] {
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
