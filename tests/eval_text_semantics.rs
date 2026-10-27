use poirot::eval::{
    EvaluationBudgets, EvaluationOutcome, RunConfiguration, run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellValue, Workbook};

fn run(formula: &str, configuration: &RunConfiguration) -> EvaluationOutcome<CellValue> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    run_formula_with_configuration(
        &workbook,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        configuration,
        EvaluationBudgets::default(),
    )
}

#[test]
fn documented_text_counts_surrogate_pairs() {
    assert!(matches!(
        run("LEN(\"😀\")", &RunConfiguration::default()),
        EvaluationOutcome::Complete {
            value: CellValue::Number(1.),
            ..
        }
    ));
}
