//! Public irregular financial contracts from independently authored cash-flow equations.
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, DateSystem, Workbook};
fn at(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn call(b: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        b,
        0,
        at("Z100").address,
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn near(value: CellValue, expected: f64) {
    let CellValue::Number(actual) = value else {
        panic!("{value:?}")
    };
    assert!(
        (actual - expected).abs() <= 3e-12 * expected.abs() + 1e-12,
        "{actual:e} != {expected:e}"
    );
}
const LONG: &str = "DATE(2023,4,1),DATE(2025,1,1),DATE(2023,1,1),DATE(2024,1,1),0.1";
#[test]
fn all_six_functions_have_independent_arithmetic_controls() {
    let b = book();
    for (formula, expected) in [
        (
            "AMORLINC(1000,DATE(2024,1,1),DATE(2024,4,1),100,0,0.2)".to_owned(),
            50.,
        ),
        (
            "AMORDEGRC(1000,DATE(2024,1,1),DATE(2024,4,1),100,1,0.2)".to_owned(),
            360.,
        ),
        (format!("ODDFPRICE({LONG},0.1,100,2)"), 99.73715049937014),
        (format!("ODDFYIELD({LONG},99.73715049937014,100,2)"), 0.1),
        (
            "ODDLPRICE(DATE(2024,4,1),DATE(2024,10,1),DATE(2024,1,1),0.1,0.1,100,2)".to_owned(),
            4195. / 42.,
        ),
        (
            "ODDLYIELD(DATE(2024,4,1),DATE(2024,10,1),DATE(2024,1,1),0.1,4195/42,100,2)".to_owned(),
            0.1,
        ),
    ] {
        near(call(&b, &formula), expected);
    }
    for basis in 0..=4 {
        near(
            call(&b, &format!("ODDFPRICE({LONG},0,100,2,{basis})")),
            match basis {
                1 => 120. - 5. * 90. / 181.,
                2 => 110. + 5. * 275. / 180.,
                3 => 110. + 5. * 275. / 182.5,
                _ => 117.5,
            },
        );
    }
}
#[test]
fn arities_required_redemption_errors_defaults_and_blanks() {
    let b = book();
    near(call(&b, "AMORLINC(1000,45000,45090,100,1.9,0.2,)"), 200.);
    near(
        call(&b, "AMORLINC(1000,45000,45090,100,1,\"0.2\",A1)"),
        200.,
    );
    near(call(&b, &format!("ODDFPRICE({LONG},0,100,2,)")), 117.5);
    for (formula, error) in [
        ("AMORLINC(1000,45000,45090,100,1,0.2,2)", CellError::Num),
        ("AMORDEGRC(1000,45000,45090,100,1,0.5)", CellError::Num),
        ("AMORLINC(1000,45090,45000,100,1,0.2)", CellError::Num),
        (
            "AMORLINC(1000,\"bad date\",45090,100,1,0.2)",
            CellError::Value,
        ),
        ("AMORLINC(1000,1/0,45090,100,1,0.2)", CellError::Div0),
        ("AMORLINC(1000,45000,45090,100,-1,0.2)", CellError::Num),
        ("AMORLINC(1000,45000,45090,100,1,0)", CellError::Num),
        ("AMORLINC(1000,45000,45090,100,1,)", CellError::Value),
        (
            "ODDFPRICE(45030,45500,45000,45180,0.1,0.1,2)",
            CellError::Value,
        ),
        ("ODDLPRICE(45030,45180,45000,0.1,0.1,,2)", CellError::Value),
        ("ODDLYIELD(45030,45180,45000,0.1,0,100,2)", CellError::Num),
        ("ODDLPRICE(45000,45180,45000,0.1,0.1,100,2)", CellError::Num),
        (
            "ODDLPRICE(45030,45180,45000,0.1,-0.1,100,2)",
            CellError::Num,
        ),
        ("ODDLPRICE(45030,45180,45000,0.1,0.1,A1,2)", CellError::Num),
        ("ODDLPRICE(45030,45180,45000,0.1,0.1,100,3)", CellError::Num),
        (
            "ODDLPRICE(45030,45180,45000,0.1,0.1,100,2,5)",
            CellError::Num,
        ),
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(error), "{formula}");
    }
}
#[test]
fn lifted_direct_overlay_dependencies_snapshot_and_spill_shrink() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 0.1).unwrap();
    s.set_formula(
        at("D1").address,
        "AMORLINC({1000;2000},DATE(2024,1,1),DATE(2024,4,1),100,1,A1)",
    )
    .unwrap();
    s.set_formula(
        at("G1").address,
        "ODDLPRICE(DATE(2024,4,1),DATE(2024,10,1),DATE(2024,1,1),{0.1;0.2},0,100,2)",
    )
    .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    near(direct.cell(at("D2")).unwrap(), 200.);
    near(direct.cell(at("G2")).unwrap(), 110.);
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        200.,
    );
    overlay.set_value(at("A1"), 0.2).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        400.,
    );
    let (b, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&b).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        400.,
    );
    restored
        .set_formula(at("D1"), "AMORLINC(1000,45000,45090,100,1,A1)")
        .unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Blank
    );
}
#[test]
fn epoch_context_arrays_lazy_branches_and_collisions_remain_host_owned() {
    for epoch in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let mut b = book();
        b.date_system = epoch;
        near(
            call(&b, &format!("ODDFPRICE({LONG},0.1,100,2)")),
            99.73715049937014,
        );
    }
    let mut b = book();
    near(
        call(
            &b,
            "IF(FALSE,AMORDEGRC(1000,45000,45090,0,100000000,1E-9),7)",
        ),
        7.,
    );
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    assert!(matches!(
        run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            "AMORLINC(1000,45000,45090,100,1,0.2)",
            &config,
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Name),
            ..
        }
    ));
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("D2").address, 9.).unwrap();
    s.set_formula(
        at("D1").address,
        "AMORLINC({1000;2000},45000,45090,100,1,0.2)",
    )
    .unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct.cell(at("D1")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
}
#[test]
fn metered_quasi_periods_and_newton_aborts_escape_iferror() {
    let b = book();
    for formula in [
        format!("IFERROR(ODDFYIELD({LONG},50,100,2,1),99)"),
        "IFERROR(ODDLPRICE(DATE(9998,1,1),DATE(9999,1,1),DATE(1900,1,1),0.1,0.1,100,4),99)".into(),
        "IFERROR(AMORDEGRC(1E12,45000,45090,0,500,0.0001),99)".into(),
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    at("Z100").address,
                    &formula,
                    Default::default(),
                    EvaluationBudgets {
                        run_work: 100,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}
