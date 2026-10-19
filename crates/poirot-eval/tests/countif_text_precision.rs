//! POI 5.4.1 probe evidence: see poirot-test-runner/docs/fods-COUNTIF_PRECISION.md.
use poirot_eval::{EvaluationOutcome, RunConfiguration, run_formula_with_configuration};
use poirot_model::{CellValue, Workbook};

#[test]
fn countif_text_precision_preserves_poi_comparison_and_raw_numeric_criteria() {
    let mut workbook = Workbook::new();
    workbook
        .add_sheet("Probe")
        .unwrap()
        .set_value("A1".parse().unwrap(), 86.47 / 86400.0)
        .unwrap();
    for configuration in [RunConfiguration::default(), RunConfiguration::default()] {
        for (formula, expected) in [
            ("A1", CellValue::Number(86.47 / 86400.0)),
            ("\"=\"&A1", CellValue::Text("=0.00100081018518519".into())),
            ("COUNTIF(A1:A1,\"=\"&A1)", CellValue::Number(0.0)),
            ("COUNTIF(A1:A1,A1)", CellValue::Number(1.0)),
        ] {
            let outcome = run_formula_with_configuration(
                &workbook,
                0,
                "B1".parse().unwrap(),
                formula,
                &configuration,
                Default::default(),
            );
            assert!(
                matches!(outcome, EvaluationOutcome::Complete { ref value, .. } if value == &expected),
                "{formula}: {outcome:?}"
            );
        }
    }
}
