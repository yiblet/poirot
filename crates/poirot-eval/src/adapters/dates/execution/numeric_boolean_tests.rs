use super::*;
#[test]
fn actual_days360_nan_is_value_infinities_are_true_and_legacy_helper_is_unchanged() {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    let c = RunConfiguration::default();
    let at = CellAddress { row: 0, col: 10 };
    let run = |f: &str| match run_formula_with_configuration(
        &w,
        0,
        at,
        f,
        &c,
        EvaluationBudgets::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    };
    let nan = "NORM.DIST(\"1.7976931348623157e308\",\"-1.7976931348623157e308\",\"1.7976931348623157e308\",TRUE)";
    assert_eq!(run(&format!("DAYS360(1,59,{nan})")), Error(VALUE_ERROR));
    assert_eq!(
        run(&format!("IFERROR(DAYS360(1,59,{nan}),42)")),
        Number(42.)
    );
    for inf in ["NORMSINV(1E-300)", "NORM.DIST(0,0,\"5e-324\",FALSE)"] {
        assert_eq!(
            run(&format!("DAYS360(1,59,{inf})")),
            run("DAYS360(1,59,TRUE)")
        );
    }
    // Legacy finite ingress cannot produce raw NaN; test its unchanged pure coercion explicitly.
    assert_eq!(coercev2_logical_boolean(&Number(f64::NAN)), Ok(true));
    assert_eq!(
        core_projection::coercev2_operand_boolean(&Number(f64::NAN)),
        Err(VALUE_ERROR)
    );
}
