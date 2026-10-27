//! Spreadsheet numeric text preserves arithmetic values across text policies.
use poirot::eval::{EvaluationOutcome, RunConfiguration, run_formula_with_configuration};
use poirot::model::{CellAddress, CellValue, Workbook};

#[test]
fn numeric_projection_uses_spreadsheet_precision_and_preserves_numeric_bits() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Projection").unwrap();
    let input = CellAddress { row: 0, col: 0 };
    let target = CellAddress { row: 0, col: 1 };
    let number = -14.000000000000002_f64;
    workbook
        .sheet_mut(0)
        .unwrap()
        .set_value(input, CellValue::Number(number))
        .unwrap();
    let legacy = RunConfiguration::default();

    for (_source, expected) in [(true, "-14")] {
        let configuration = legacy.clone();
        for formula in ["A1&\"\"", "CONCAT(A1)", "CONCATENATE(A1)"] {
            let outcome = run_formula_with_configuration(
                &workbook,
                0,
                target,
                formula,
                &configuration,
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
            target,
            "A1",
            &configuration,
            Default::default(),
        );
        assert!(
            matches!(outcome,EvaluationOutcome::Complete { value:CellValue::Number(value),.. } if value.to_bits()==number.to_bits())
        );
        assert!(
            matches!(workbook.sheet(0).unwrap().get(input).unwrap().unwrap().value,CellValue::Number(value) if value.to_bits()==number.to_bits())
        );
    }
}
