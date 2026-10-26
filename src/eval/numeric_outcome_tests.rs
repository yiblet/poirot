use super::*;
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w
}
fn at(col: u32) -> CellAddress {
    CellAddress { row: 0, col }
}

fn run(w: &Workbook, formula: &str) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(
        w,
        0,
        at(25),
        formula,
        &RunConfiguration::default(),
        EvaluationBudgets::default(),
    )
}
fn complete(outcome: EvaluationOutcome<CellValue>) -> CellValue {
    match outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn bits(value: CellValue) -> u64 {
    match value {
        Number(n) => n.to_bits(),
        other => panic!("{other:?}"),
    }
}
#[test]
fn inverse_spellings_validate_probability_boundaries() {
    let w = book();
    {
        for f in [
            "NORMSINV(0)",
            "NORM.S.INV(1)",
            "NORMINV(0,0,1)",
            "NORM.INV(1,0,1)",
        ] {
            assert_eq!(complete(run(&w, f)), Error(Num));
        }
        assert_eq!(bits(complete(run(&w, "NORMSINV(0.5)"))), 0.0f64.to_bits());
    }
}
#[test]
fn construction_stages_flush_both_subnormal_signs_preserve_existing_negative_zero() {
    for raw in [1, 0x8000_0000_0000_0001] {
        let mut w = book();
        w.sheet_mut(0)
            .unwrap()
            .set_value(at(0), Number(f64::from_bits(raw)))
            .unwrap();
        assert_eq!(bits(complete(run(&w, "A1"))), 0);
        assert_eq!(complete(run(&w, "NORMSINV(A1)")), Error(Num));
    }
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(-0.0))
        .unwrap();
    assert_eq!(bits(complete(run(&w, "A1"))), (-0.0f64).to_bits());
    assert_eq!(bits(complete(run(&w, "-A1"))), 0);
    assert_eq!(bits(complete(run(&w, "A1*1"))), 0);
    assert_eq!(bits(complete(run(&w, "A1-0"))), (-0.0f64).to_bits());
    assert_eq!(complete(run(&w, "A1<0")), Boolean(true));
    assert_eq!(bits(complete(run(&w, "2.2250738585072014E-308/2"))), 0);
    assert_eq!(bits(complete(run(&w, "2.2250738585072014E-308%"))), 0);
    assert_eq!(
        complete(run(&w, "NORMSINV(4.9406564584124654E-324)")),
        Error(Num)
    );
    for mean in ["4.9406564584124654E-324", "-4.9406564584124654E-324"] {
        let f = format!("NORM.INV(0.5,\"{mean}\",1)");
        assert_eq!(
            bits(complete(run(&w, &f))),
            0,
            "function-result NumberEval flushes either sign"
        );
    }
    // NumberPtg token gate differs from a stored numeric-cell read.
    let mut session = Session::new(
        &w,
        EvaluationInputs::default(),
        BudgetMeter::new(EvaluationBudgets::default()).unwrap(),
    );

    let admitted = session
        .expr(0, at(1), &Expr::Number(f64::INFINITY))
        .unwrap();
    assert_eq!(
        session.select_legacy_scalar(at(1), admitted).unwrap(),
        Error(Num)
    );
    assert_eq!(
        bits(numeric_outcome::coerce_numeric_outcome(f64::from_bits(1))),
        0
    );
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::INFINITY))
        .unwrap();
    assert_eq!(bits(complete(run(&w, "A1"))), f64::INFINITY.to_bits());
}
#[test]
fn retained_overlay_normalization_keeps_raw_source_dependency_identity() {
    use crate::eval::overlay::{EvaluationContext, EvaluationPolicy, Overlay};
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::from_bits(1)))
        .unwrap();
    w.sheet_mut(0).unwrap().set_formula(at(1), "A1").unwrap();
    let mut overlay = Overlay::with_workbook(&w)
        .with_policy(EvaluationPolicy {
            ..Default::default()
        })
        .build()
        .unwrap();
    let input = overlay.target("S!A1").unwrap();
    let output = overlay.target("S!B1").unwrap();
    let context = EvaluationContext::default();
    assert_eq!(
        bits(overlay.evaluation(&context).unwrap().cell(output).unwrap()),
        0
    );
    for raw in [2, 0, 0x8000_0000_0000_0000] {
        overlay
            .set_value(input, Number(f64::from_bits(raw)))
            .unwrap();
        assert!(overlay.cached_value(output).is_none());
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let expected = if raw == 0x8000_0000_0000_0000 { raw } else { 0 };
        assert_eq!(bits(evaluation.cell(output).unwrap()), expected);
        assert_eq!(evaluation.usage().formula_executions, 1);
    }
}
