//! Actual TEXT uses the explicit deterministic system-format policy.
use poirot_eval::{
    EvaluationOutcome, RunConfiguration, TextSemanticsPolicy, TextSourceVersion,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellValue, Workbook};
#[test]
fn text_system_date_and_time_are_configured_display_not_changed_values() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Date").unwrap();
    let default = RunConfiguration::default();
    let poi = default.clone().with_text_policy(TextSemanticsPolicy {
        source: TextSourceVersion::Poi7321209,
    });
    for configuration in [&default, &poi] {
        for (formula, expected) in [
            ("TEXT(46082,\"[$-F800]\")", "Sunday, March 1, 2026"),
            (
                "TEXT(46447,\"[$-F800]yyyy-mm-dd\")",
                "Monday, March 1, 2027",
            ),
            ("TEXT(46082.5,\"[$-F400]\")", "12:00:00 PM"),
        ] {
            let outcome = run_formula_with_configuration(
                &workbook,
                0,
                CellAddress { row: 0, col: 0 },
                formula,
                configuration,
                Default::default(),
            );
            assert!(
                matches!(outcome,EvaluationOutcome::Complete { value,.. } if value==CellValue::Text(expected.into())),
                "{formula}"
            );
        }
        let outcome = run_formula_with_configuration(
            &workbook,
            0,
            CellAddress { row: 0, col: 0 },
            "46082",
            configuration,
            Default::default(),
        );
        assert!(
            matches!(outcome,EvaluationOutcome::Complete { value:CellValue::Number(value),.. } if value.to_bits()==46082_f64.to_bits())
        );
    }
}
