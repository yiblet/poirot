//! Public inverse contracts derived from elementary integrals and counting.
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, Workbook};
fn target(cell: &str) -> EvaluationTarget {
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
fn call(b: &Workbook, f: &str) -> CellValue {
    match run_formula(
        b,
        0,
        target("Z100").address,
        f,
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
        (actual - expected).abs()
            <= if expected == 0.0 {
                1e-15
            } else {
                expected.abs() * 3e-11
            },
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn all_twelve_names_execute_independent_equations() {
    let b = book();
    for (f, e) in [
        ("BETA.INV(0.25,2,1)", 0.5),
        ("BINOM.INV(4,0.5,5/16)", 1.),
        ("CHISQ.INV(0.5,2)", 2. * 2f64.ln()),
        ("CHISQ.INV.RT(0.25,2)", 2. * 4f64.ln()),
        ("CONFIDENCE.NORM(0.05,1,1)", 1.9599639845400542),
        ("CONFIDENCE.T(0.5,2,2)", 2f64.sqrt()),
        ("F.INV(0.25,2,2)", 1. / 3.),
        ("F.INV.RT(0.25,2,2)", 3.),
        ("GAMMA.INV(0.5,1,2)", 2. * 2f64.ln()),
        ("LOGNORM.INV(0.5,LN(3),7)", 3.),
        ("T.INV(0.75,2)", (2f64 / 3.).sqrt()),
        ("T.INV.2T(0.5,1)", 1.),
    ] {
        near(call(&b, f), e);
    }
}
#[test]
fn endpoints_defaults_counts_origins_and_errors() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(target("A1").address, "0.5").unwrap();
    s.set_value(target("A2").address, 0.5).unwrap();
    s.set_value(target("A3").address, true).unwrap();
    near(call(&b, "BETA.INV(A1,1,1,,)"), 0.5);
    near(call(&b, "BETA.INV(A2,1,1,2,6)"), 4.);
    near(call(&b, "BETA.INV(0.5,1,1,A4,)"), 0.5);
    near(call(&b, "BETA.INV(A3,1,1)"), 1.);
    near(call(&b, "BINOM.INV(4.9,0.5,0.5)"), 2.);
    near(call(&b, "BINOM.INV(0,0.5,0.5)"), 0.);
    near(call(&b, "CHISQ.INV(0,2)"), 0.);
    near(call(&b, "CHISQ.INV.RT(1,2)"), 0.);
    near(call(&b, "F.INV(0,2,2)"), 0.);
    near(call(&b, "F.INV.RT(1,2,2)"), 0.);
    near(call(&b, "GAMMA.INV(0,1,2)"), 0.);
    near(call(&b, "T.INV.2T(1,1.9)"), 0.);
    near(call(&b, "CONFIDENCE.T(0.5,2,2.9)"), 2f64.sqrt());
    for (f, e) in [
        ("BETA.INV(0,1,1)", CellError::Num),
        ("BETA.INV(0.5,1,1,0,A4)", CellError::Num),
        ("BINOM.INV(4,0,0.5)", CellError::Num),
        ("BINOM.INV(4,1,0.5)", CellError::Num),
        ("BINOM.INV(4,0.5,0)", CellError::Num),
        ("BINOM.INV(4,0.5,1)", CellError::Num),
        ("CHISQ.INV(0.5,10000000001)", CellError::Num),
        ("CHISQ.INV(1,2)", CellError::Num),
        ("CHISQ.INV.RT(0,2)", CellError::Num),
        ("F.INV.RT(0.5,2,10000000000)", CellError::Num),
        ("F.INV(1,2,2)", CellError::Num),
        ("GAMMA.INV(0.5,1,0)", CellError::Num),
        ("GAMMA.INV(A1,1,2)", CellError::Value),
        ("GAMMA.INV(\"0.5\",1,2)", CellError::Value),
        ("LOGNORM.INV(0,0,1)", CellError::Num),
        ("LOGNORM.INV(1,0,1)", CellError::Num),
        ("T.INV(1,1)", CellError::Num),
        ("T.INV.2T(0,1)", CellError::Num),
        ("CONFIDENCE.T(0.5,1,1.9)", CellError::Div0),
        ("CONFIDENCE.NORM(0.5,1,0.9)", CellError::Num),
        ("BETA.INV(1/0)", CellError::Value),
        ("GAMMA.INV(1/0,0,0)", CellError::Div0),
        ("BETA.INV(0.5,0,1,1/0)", CellError::Div0),
    ] {
        assert_eq!(call(&b, f), CellValue::Error(e), "{f}");
    }
}
#[test]
fn every_name_lifts_in_direct_overlay_and_current_snapshots() {
    let mut b = book();
    // Both output members have independently known elementary values.
    let cases = [
        ("BETA.INV({0.25;0.5},1,1)", [0.25, 0.5]),
        ("BINOM.INV(4,0.5,{0.3125;0.5})", [1., 2.]),
        (
            "CHISQ.INV({0.25;0.5},2)",
            [-2. * 0.75f64.ln(), 2. * 2f64.ln()],
        ),
        (
            "CHISQ.INV.RT({0.25;0.5},2)",
            [2. * 4f64.ln(), 2. * 2f64.ln()],
        ),
        (
            "CONFIDENCE.NORM(0.05,{1;2},1)",
            [1.9599639845400542, 3.9199279690801085],
        ),
        ("CONFIDENCE.T(0.5,{1;2},2)", [1. / 2f64.sqrt(), 2f64.sqrt()]),
        ("F.INV({0.25;0.5},2,2)", [1. / 3., 1.]),
        ("F.INV.RT({0.25;0.5},2,2)", [3., 1.]),
        (
            "GAMMA.INV({0.25;0.5},1,2)",
            [-2. * 0.75f64.ln(), 2. * 2f64.ln()],
        ),
        ("LOGNORM.INV(0.5,{0;1},1)", [1., 1f64.exp()]),
        ("T.INV({0.25;0.75},1)", [-1., 1.]),
        ("T.INV.2T({0.5;1},1)", [1., 0.]),
    ];
    for (i, (formula, _)) in cases.iter().enumerate() {
        b.sheet_mut(0)
            .unwrap()
            .set_formula(
                poirot::model::CellAddress {
                    row: 0,
                    col: (i * 3) as u32,
                },
                *formula,
            )
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    for (i, (_, expected)) in cases.iter().enumerate() {
        for (row, e) in expected.iter().enumerate() {
            let t = EvaluationTarget {
                sheet: 0,
                address: poirot::model::CellAddress {
                    row: row as u32,
                    col: (i * 3) as u32,
                },
            };
            near(direct.cell(t).unwrap(), *e);
            near(overlay.evaluation(&context).unwrap().cell(t).unwrap(), *e);
        }
    }
    let (source, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&source).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("AH2"))
            .unwrap(),
        0.,
    );
}
#[test]
fn rectangular_broadcast_edits_spill_removal_and_collision() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(target("A1").address, 2.).unwrap();
    s.set_formula(target("D1").address, "BETA.INV({0.25;0.5},1,1,0,{1,2}*A1)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E2"))
            .unwrap(),
        2.,
    );
    overlay.set_value(target("A1"), 4.).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E2"))
            .unwrap(),
        4.,
    );
    overlay
        .set_formula(target("D1"), "BETA.INV(0.25,1,1,0,A1)")
        .unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        1.,
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("E2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(target("D2"), 7.).unwrap();
    overlay
        .set_formula(target("D1"), "BETA.INV({0.25;0.5},1,1)")
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
}
#[test]
fn tiny_tail_large_shape_and_scale_controls() {
    let b = book();
    let CellValue::Number(scaled) = call(&b, "BETA.INV(1E-300,0.5,1,0,1E308)") else {
        panic!()
    };
    assert!((scaled / 1e-292 - 1.).abs() < 3e-13);
    near(call(&b, "F.INV.RT(1E-20,2,2)"), 1e20);
    near(call(&b, "CHISQ.INV(1E-20,2)"), 2e-20);
    near(call(&b, "LOGNORM.INV(1E-20,0,1)"), 9.493291347224353e-5);
    near(
        call(&b, "GAMMA.INV(1E-300,0.5,1E300)"),
        std::f64::consts::PI * 0.25 * 1e-300,
    );
    near(
        call(&b, "CHISQ.INV.RT(0.4999981193680548,10000000000)"),
        1e10,
    );
}
#[test]
fn laziness_pinned_availability_and_uncatchable_work_failures() {
    let b = book();
    near(
        call(&b, "IF(FALSE,BETA.INV(0.3,2000000000,1000000000),7)"),
        7.,
    );
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    assert!(matches!(
        run_formula_with_configuration(
            &b,
            0,
            target("Z100").address,
            "T.INV(0.75,2)",
            &config,
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Name),
            ..
        }
    ));
    for formula in [
        "IFERROR(BETA.INV(0.3,2000000000,1000000000),99)",
        "IFERROR(CHISQ.INV(0.3,10000000000),99)",
        "IFERROR(BINOM.INV(1E13,0.5,0.5),99)",
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    target("Z100").address,
                    formula,
                    Default::default(),
                    EvaluationBudgets {
                        run_work: 50,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{formula}"
        );
    }
}

#[test]
fn flat_cdf_center_and_subnormal_factors_preserve_final_quantiles() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(target("A1").address, f64::from_bits(1))
        .unwrap();
    near(
        call(&b, "BETA.INV(0.49999999999999,1E-8,1E-8)"),
        0.499999500399632,
    );
    let CellValue::Number(g) = call(&b, "GAMMA.INV(1E-161,0.5,1E308)") else {
        panic!()
    };
    assert!((g / (std::f64::consts::PI * 0.25 * 1e-14) - 1.).abs() < 3e-13);
    // Source ingestion flushes subnormals before adapters; the pure tests separately
    // exercise the minimum binary64 input. A normal sigma can still create a
    // subnormal standard error internally, followed by a normal final margin.
    assert_eq!(
        call(&b, "CONFIDENCE.T(1E-300,A1,2)"),
        CellValue::Error(CellError::Num)
    );
    let CellValue::Number(c) = call(&b, "CONFIDENCE.T(1E-300,2.2250738585072014E-308,2)") else {
        panic!()
    };
    let expected = (2. / std::f64::consts::PI) * (f64::MIN_POSITIVE / 1e-300) / 2f64.sqrt();
    assert!((c / expected - 1.).abs() < 3e-13);
}

#[test]
fn each_lifted_cell_pays_for_its_inverse_work() {
    let b = book();
    let single = run_formula(
        &b,
        0,
        target("Z100").address,
        "BETA.INV(0.3,2.3,4.7)",
        Default::default(),
        Default::default(),
    );
    let array = run_formula(
        &b,
        0,
        target("Z100").address,
        "BETA.INV({0.3;0.3},2.3,4.7)",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        single,
        EvaluationOutcome::Complete {
            value: CellValue::Number(_),
            ..
        }
    ));
    assert!(matches!(array, EvaluationOutcome::Complete { .. }));
    assert!(array.usage().work_units > single.usage().work_units * 18 / 10);
    let limited = run_formula(
        &b,
        0,
        target("Z100").address,
        "BETA.INV({0.3;0.3},2.3,4.7)",
        Default::default(),
        EvaluationBudgets {
            run_work: single.usage().work_units + 1,
            ..Default::default()
        },
    );
    assert!(matches!(limited, EvaluationOutcome::Aborted { .. }));
}
