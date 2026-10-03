//! Behavioral obligations migrated from the retired mutable transaction cache.
use poirot_eval::{
    overlay::{EvaluationContext, Overlay, OverlayPolicy},
    *,
};
use poirot_model::{CellValue, Workbook};
use std::{
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
fn target(col: u32) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: poirot_model::CellAddress { row: 0, col },
    }
}
fn book(formulas: &[&str]) -> Workbook {
    let mut w = Workbook::new();
    let s = w.add_sheet("S").unwrap();
    for (col, formula) in formulas.iter().enumerate() {
        s.set_formula(target(col as u32).address, *formula, CellValue::Blank)
            .unwrap();
    }
    w
}
fn context() -> EvaluationContext {
    EvaluationContext {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1_609_502_400),
        ..Default::default()
    }
}
#[test]
fn formatting_changes_invalidate_only_executed_consumers() {
    let w = book(&["DOLLAR(1)", "2+3", "IF(FALSE,DOLLAR(2),7)"]);
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let mut currency = CurrencyContext {
        decimal_separator: ".".into(),
        grouping_separator: ",".into(),
        primary_group: 3,
        secondary_group: 3,
        positive_prefix: "$".into(),
        positive_suffix: "".into(),
        negative_prefix: "($".into(),
        negative_suffix: ")".into(),
    };
    let mut context = context();
    context.formatting = FormattingConfiguration::with_currency(currency.clone());
    let targets = [target(0), target(1), target(2)];
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(first[0].value, CellValue::Text("$1.00".into()));
    let untouched = overlay.cached_value(target(2)).unwrap();
    currency.positive_prefix = "USD ".into();
    context.formatting = FormattingConfiguration::with_currency(currency);
    let mut evaluation = overlay.evaluation(&context).unwrap();
    let values = evaluation.outputs(&targets).unwrap();
    assert_eq!(values[0].value, CellValue::Text("USD 1.00".into()));
    assert_eq!(evaluation.usage().formula_executions, 1);
    drop(evaluation);
    assert!(Arc::ptr_eq(
        &untouched,
        &overlay.cached_value(target(2)).unwrap()
    ));
}
#[test]
fn localized_date_context_refreshes_actual_consumers_and_preserves_constants() {
    let w = book(&[
        "DATE(2020,1,2)",
        "NOW()",
        "DATEVALUE(\"1/2/2020\")",
        "DATEVALUE(\"1/2\")",
        "TIMEVALUE(\"12:30\")",
    ]);
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let mut context = context();
    let targets = (0..5).map(target).collect::<Vec<_>>();
    overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    let constant = overlay.cached_value(target(0)).unwrap();
    let explicit_year = overlay.cached_value(target(2)).unwrap();
    let time_only = overlay.cached_value(target(4)).unwrap();
    context.as_of_time += Duration::from_secs(86400);
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&targets).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 2);
    drop(evaluation);
    assert!(Arc::ptr_eq(
        &constant,
        &overlay.cached_value(target(0)).unwrap()
    ));
    assert!(Arc::ptr_eq(
        &explicit_year,
        &overlay.cached_value(target(2)).unwrap()
    ));
    context.localization.date_order = DateOrder::DayMonthYear;
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&targets).unwrap();
    // NOW is volatile in every canonical evaluation, independently of context changes.
    assert_eq!(evaluation.usage().formula_executions, 3);
    drop(evaluation);
    assert!(Arc::ptr_eq(
        &time_only,
        &overlay.cached_value(target(4)).unwrap()
    ));
}
#[test]
fn yearless_numeric_coercion_tracks_exact_clock_and_failure_never_changes_source() {
    let w = book(&["ABS(\"1 Jan\")"]);
    let before = w.clone();
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let mut context = context();
    context.as_of_time = UNIX_EPOCH + Duration::from_secs(1_704_067_200);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .unwrap(),
        CellValue::Number(45292.)
    );
    context.as_of_time = UNIX_EPOCH + Duration::from_secs(1_735_689_600);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .unwrap(),
        CellValue::Number(45658.)
    );
    context.as_of_time = UNIX_EPOCH + Duration::from_secs(300_000_000_000);
    assert!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .is_err()
    );
    assert_eq!(w, before);
}
#[test]
fn policy_switch_reexecutes_text_consumers_and_preserves_split_surrogate_units() {
    let w = book(&[
        "LOWER(\"ASCII\")",
        "LEFT(\"😀\",1)",
        "IF(FALSE,LOWER(\"Ä\"),7)",
        "2+3",
    ]);
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(OverlayPolicy {
            text: TextSemanticsPolicy {
                source: TextSourceVersion::Poi7321209,
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let context = context();
    let targets = (0..4).map(target).collect::<Vec<_>>();
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(first[0].value, CellValue::Text("ascii".into()));
    assert_eq!(
        first[1]
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![0xd83d]
    );
    let mut policy = overlay.policy().clone();
    policy.text.source = TextSourceVersion::Poi7321209OpenJdk21Unicode15;
    overlay.set_policy(policy).unwrap();
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(
        values[1]
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![0xd83d]
    );
    assert_eq!(values[2].value, CellValue::Number(7.));
    assert_eq!(values[3].value, CellValue::Number(5.));
}
#[test]
fn cached_split_surrogate_budget_failure_is_operational_and_cannot_be_masked() {
    let w = book(&["IFERROR(LEFT(\"😀\",1),7)"]);
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(OverlayPolicy {
            text: TextSemanticsPolicy {
                source: TextSourceVersion::Poi7321209,
            },
            ..Default::default()
        })
        .build()
        .unwrap();
    let mut context = context();
    let value = overlay
        .evaluation(&context)
        .unwrap()
        .cell(target(0))
        .unwrap();
    assert_eq!(
        value.text_ref().unwrap().units().collect::<Vec<_>>(),
        vec![0xd83d]
    );
    let committed = overlay.changes();
    context.budgets.text_bytes = 0;
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(target(0)),
        Err(EvaluationFailure::Budget(_))
    ));
    assert_eq!(overlay.changes(), committed);
}
#[test]
fn selected_branch_replacement_discards_obsolete_dependencies() {
    let mut w = book(&["IF(D1,B1,C1)", "2+3", "7+8"]);
    w.sheet_mut(0)
        .unwrap()
        .set_value(target(3).address, true)
        .unwrap();
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let context = context();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .unwrap(),
        CellValue::Number(5.)
    );
    overlay.set_value(target(3), false).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .unwrap(),
        CellValue::Number(15.)
    );
    let retained = overlay.cached_value(target(0)).unwrap();
    overlay.set_formula(target(1), "1/0").unwrap();
    assert!(Arc::ptr_eq(
        &retained,
        &overlay.cached_value(target(0)).unwrap()
    ));
    let mut evaluation = overlay.evaluation(&context).unwrap();
    assert_eq!(evaluation.cell(target(0)).unwrap(), CellValue::Number(15.));
    assert_eq!(evaluation.usage().formula_executions, 0);
}

#[test]
fn documented_text_version_switch_updates_index_consumers_and_dependents() {
    let w = book(&[
        "LEN(\"😀\")",
        "REPLACE(\"😀\",1,1,\"X\")",
        "LEN(B1)",
        "IF(FALSE,REPLACE(\"😀\",1,1,\"X\"),7)",
    ]);
    let mut policy = OverlayPolicy {
        text: TextSemanticsPolicy {
            source: TextSourceVersion::Poi7321209,
        },
        ..Default::default()
    };
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(policy.clone())
        .build()
        .unwrap();
    let context = context();
    let targets = (0..4).map(target).collect::<Vec<_>>();
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(first[0].value, CellValue::Number(2.));
    assert_eq!(
        first[1]
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![88, 0xde00]
    );
    policy.text.source = TextSourceVersion::ExcelCompatibilityV2Documented;
    overlay.set_policy(policy.clone()).unwrap();
    let changed = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(changed[0].value, CellValue::Number(1.));
    assert_eq!(changed[1].value, CellValue::Text("X".into()));
    assert_eq!(changed[2].value, CellValue::Number(1.));
    assert_eq!(changed[3].value, CellValue::Number(7.));
    policy.text.source = TextSourceVersion::Poi7321209;
    overlay.set_policy(policy).unwrap();
    let restored = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(
        restored[1]
            .value
            .text_ref()
            .unwrap()
            .units()
            .collect::<Vec<_>>(),
        vec![88, 0xde00]
    );
}

#[test]
fn builtin_policy_switch_recomputes_semantics_and_unchanged_policy_reuses() {
    let w = book(&["PERCENTILE.INC({1,3},0.5)", "A1+1", "QUARTILE.INC({1,3},2)"]);
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let context = context();
    let targets = (0..3).map(target).collect::<Vec<_>>();
    let first = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(
        first[0].value,
        CellValue::Error(poirot_model::CellError::Name)
    );
    let mut policy = overlay.policy().clone();
    policy.builtin = BuiltinSemanticsPolicy::ExcelDocumentation;
    overlay.set_policy(policy.clone()).unwrap();
    let values = overlay
        .evaluation(&context)
        .unwrap()
        .outputs(&targets)
        .unwrap();
    assert_eq!(
        values.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
        vec![
            CellValue::Number(2.),
            CellValue::Number(3.),
            CellValue::Number(2.)
        ]
    );
    overlay.set_policy(policy).unwrap();
    let mut evaluation = overlay.evaluation(&context).unwrap();
    evaluation.outputs(&targets).unwrap();
    assert_eq!(evaluation.usage().formula_executions, 0);
}

#[test]
fn text_coercion_policy_switch_exposes_capability_failure_without_source_publication() {
    let w = book(&["IFERROR(REPT(\"x\",\"1/02\"),7)"]);
    let before = w.clone();
    let mut overlay = Overlay::with_workbook(&w).build().unwrap();
    let mut context = context();
    context.as_of_time = UNIX_EPOCH + Duration::from_secs(300_000_000_000);
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target(0))
            .unwrap(),
        CellValue::Number(7.)
    );
    let mut policy = overlay.policy().clone();
    policy.text.source = TextSourceVersion::Poi7321209;
    overlay.set_policy(policy).unwrap();
    assert!(matches!(
        overlay.evaluation(&context).unwrap().cell(target(0)),
        Err(EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::DateParsing,
            ..
        })
    ));
    assert_eq!(w, before);
}
