use super::*;
#[test]
fn days360_finite_method_selectors_and_selected_errors() {
    // Both documented 30/360 methods put exactly 30 days between these month starts.
    // https://support.microsoft.com/en-us/excel/functions/days360-function
    let mut workbook = Workbook::new();
    workbook.add_sheet("S").unwrap();
    for (method, expected) in [
        ("TRUE", Number(30.)),
        ("FALSE", Number(30.)),
        ("1", Number(30.)),
        ("0", Number(30.)),
        ("", Number(30.)),
        ("#N/A", Error(NA)),
        ("1/0", Error(Div0)),
    ] {
        let formula = format!("DAYS360(DATE(2024,1,1),DATE(2024,2,1),{method})");
        match run_formula(
            &workbook,
            0,
            CellAddress { row: 0, col: 10 },
            &formula,
            Default::default(),
            Default::default(),
        ) {
            EvaluationOutcome::Complete { value, .. } => assert_eq!(value, expected, "{formula}"),
            other => panic!("{other:?}"),
        }
    }
}
