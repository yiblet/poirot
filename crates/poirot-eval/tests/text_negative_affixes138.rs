//! Actual TEXT bridge regressions for retained Output!D304 sign placement.
use poirot_eval::{
    BudgetKind, EvaluationBudgets, EvaluationFailure, EvaluationOutcome, RunConfiguration,
    TextSemanticsPolicy, TextSourceVersion, run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellValue, ExcelText, Workbook};

fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}

#[test]
fn text_numeric_affixes_match_in_default_and_pinned_poi_configuration() {
    let workbook = workbook();
    let default = RunConfiguration::default();
    let poi = default.clone().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for configuration in [&default, &poi] {
        for (formula, expected) in [
            (r#"TEXT(-6653,"$###,###")"#, "-$6,653"),
            (r#"TEXT(6653,"$###,###")"#, "$6,653"),
            (r#"TEXT(0,"$0")"#, "$0"),
            (r#"TEXT(-0,"$0")"#, "$0"),
            (r##"TEXT(-6653,"#,##0$")"##, "-6,653$"),
            (r#"TEXT(-6653,"$#,##0;($#,##0)")"#, "($6,653)"),
            (r#"TEXT(-6653,"$#,##0;$-#,##0")"#, "$-6,653"),
            (r#"TEXT(-2,"\-0")"#, "--2"),
            (r#"TEXT(-2,"(0)")"#, "-(2)"),
            (r#"TEXT(-0.125,"$0.0%")"#, "-$12.5%"),
            (r#"TEXT(-12345,"$0.00E+00")"#, "-$1.23E+04"),
            (r#"TEXT(-1250000,"$0.00,,")"#, "-$1.25"),
        ] {
            match run_formula_with_configuration(
                &workbook,
                0,
                CellAddress { row: 0, col: 0 },
                formula,
                configuration,
                EvaluationBudgets::default(),
            ) {
                EvaluationOutcome::Complete { value, .. } => {
                    assert_eq!(value, CellValue::Text(expected.into()), "{formula}")
                }
                outcome => panic!("{formula}: {outcome:?}"),
            }
        }
    }
}

#[test]
fn text_format_budget_and_lossless_failures_are_not_caught_by_iferror() {
    let mut workbook = workbook();
    let configuration = RunConfiguration::default();
    let syntax =
        poirot_parsing::formula::parse_formula(r#"IFERROR(TEXT(-6653,"$###,###"),7)"#).unwrap();
    let plan = poirot_eval::bound::FormulaPlan::bind(&syntax).unwrap();
    let address = CellAddress { row: 0, col: 1 };
    let outcome = poirot_eval::run_plan_with_configuration(
        &workbook,
        0,
        address,
        &plan,
        &configuration,
        EvaluationBudgets {
            text_bytes: 16,
            ..Default::default()
        },
    );
    assert!(
        matches!(outcome, EvaluationOutcome::Aborted {
        failure: EvaluationFailure::Budget(ref failure), ..
    } if failure.kind == BudgetKind::TextBytes),
        "{outcome:?}"
    );
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(
            CellAddress { row: 0, col: 0 },
            CellValue::TextUtf16(ExcelText::from_units(vec![0xd800])),
        )
        .unwrap();
    let outcome = run_formula_with_configuration(
        &workbook,
        0,
        address,
        r#"IFERROR(TEXT(-6653,A1),7)"#,
        &configuration,
        Default::default(),
    );
    assert!(
        matches!(
            outcome,
            EvaluationOutcome::Aborted {
                failure: EvaluationFailure::UnsupportedCapability { .. },
                ..
            }
        ),
        "{outcome:?}"
    );
}
