//! Modern probability contracts derived from counting and elementary integrals.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
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
        (actual - expected).abs() <= expected.abs() * 2e-11 + 1e-15,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn all_eighteen_names_execute_independent_equations() {
    let b = book();
    for (f, e) in [
        ("BETA.DIST(4,1,1,TRUE,2,6)", 0.5),
        ("BETA.DIST(0.5,2,2,FALSE)", 1.5),
        ("BINOM.DIST(2,4,0.5,FALSE)", 6. / 16.),
        ("BINOM.DIST.RANGE(4,0.5,1,3)", 14. / 16.),
        ("CHISQ.DIST(2,2,TRUE)", 1. - (-1f64).exp()),
        ("CHISQ.DIST.RT(2,2)", (-1f64).exp()),
        ("EXPON.DIST(2,0.5,FALSE)", (-1f64).exp() / 2.),
        ("F.DIST(3,2,2,TRUE)", 0.75),
        ("F.DIST.RT(3,2,2)", 0.25),
        ("GAMMA(-0.5)", -2. * std::f64::consts::PI.sqrt()),
        ("GAMMA.DIST(2,1,2,FALSE)", (-1f64).exp() / 2.),
        ("GAMMALN(5)", 24f64.ln()),
        ("GAMMALN.PRECISE(5)", 24f64.ln()),
        ("GAUSS(0)", 0.),
        ("HYPGEOM.DIST(1,2,2,6,FALSE)", 8. / 15.),
        ("LOGNORM.DIST(1,0,1,TRUE)", 0.5),
        ("NEGBINOM.DIST(2,1,0.5,TRUE)", 7. / 8.),
        ("PHI(0)", 1. / std::f64::consts::TAU.sqrt()),
        ("WEIBULL.DIST(2,1,2,TRUE)", 1. - (-1f64).exp()),
    ] {
        near(call(&b, f), e);
    }
}
#[test]
fn domains_defaults_counts_and_scalar_origin_contracts() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(target("A1").address, 0.5).unwrap();
    s.set_value(target("A2").address, "0.5").unwrap();
    s.set_value(target("A3").address, true).unwrap();
    near(call(&b, "BETA.DIST(A1,1,1,TRUE,,)"), 0.5);
    near(call(&b, "BETA.DIST(A2,1,1,TRUE)"), 0.5);
    near(call(&b, "BINOM.DIST(2.9,4.9,0.5,FALSE)"), 6. / 16.);
    near(call(&b, "BINOM.DIST.RANGE(4,0.5,2)"), 6. / 16.);
    near(call(&b, "BINOM.DIST.RANGE(4,0.5,0,A4)"), 1. / 16.);
    near(call(&b, "BETA.DIST(A3,1,1,TRUE)"), 1.);
    near(call(&b, "EXPON.DIST(A4,1,TRUE)"), 0.);
    for (f, e) in [
        ("BETA.DIST(0.5,0,1,TRUE)", CellError::Num),
        ("BETA.DIST(2,1,1,TRUE)", CellError::Num),
        ("BETA.DIST(0,1,1,TRUE,0,A4)", CellError::Num),
        ("BINOM.DIST(5,4,0.5,TRUE)", CellError::Num),
        ("CHISQ.DIST(1,10000000001,TRUE)", CellError::Num),
        ("CHISQ.DIST.RT(-1,2)", CellError::Num),
        ("F.DIST(1,0,2,TRUE)", CellError::Num),
        ("GAMMA(0)", CellError::Num),
        ("GAMMALN(-1)", CellError::Num),
        ("HYPGEOM.DIST(0,4,5,6,TRUE)", CellError::Num),
        ("NEGBINOM.DIST(0,0,0.5,TRUE)", CellError::Num),
        ("LOGNORM.DIST(0,0,1,TRUE)", CellError::Num),
        ("WEIBULL.DIST(1,0,1,FALSE)", CellError::Num),
        ("PHI(\"bad\")", CellError::Value),
        ("EXPON.DIST(1,1,\"bad\")", CellError::Value),
        ("BETA.DIST(1/0)", CellError::Value),
        ("BINOM.DIST(0,0,0,1/0)", CellError::Div0),
    ] {
        assert_eq!(call(&b, f), CellValue::Error(e), "{f}");
    }
}
#[test]
fn direct_overlay_arrays_source_edits_and_snapshot_recovery() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(target("A1").address, 0.5).unwrap();
    s.set_formula(target("D1").address, "BINOM.DIST({0;1;2},2,A1,FALSE)")
        .unwrap();
    s.set_formula(target("G1").address, "BETA.DIST({0.25,0.5},1,1,TRUE)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    near(direct.cell(target("D2")).unwrap(), 0.5);
    near(direct.cell(target("H1")).unwrap(), 0.5);
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D3"))
            .unwrap(),
        0.25,
    );
    overlay.set_value(target("A1"), 0.25).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D3"))
            .unwrap(),
        0.0625,
    );
    let (b, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&b).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("D2"))
            .unwrap(),
        0.375,
    );
    restored
        .set_formula(target("D1"), "BINOM.DIST(0,2,A1,FALSE)")
        .unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("D3"))
            .unwrap(),
        CellValue::Blank
    );
}
#[test]
fn host_owns_lazy_calls_profile_spills_and_resource_failures() {
    let mut b = book();
    near(
        call(&b, "IF(FALSE,GAMMA.DIST(10000000000,5000000000,2,TRUE),7)"),
        7.,
    );
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    assert!(matches!(
        run_formula_with_configuration(
            &b,
            0,
            target("Z100").address,
            "PHI(0)",
            &config,
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Name),
            ..
        }
    ));
    for f in [
        "IFERROR(CHISQ.DIST(10000000000,10000000000,TRUE),99)",
        "IFERROR(HYPGEOM.DIST(100,1000000,50000000,100000000,TRUE),99)",
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    target("Z100").address,
                    f,
                    Default::default(),
                    EvaluationBudgets {
                        run_work: 50,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ),
            "{f}"
        );
    }
    b.sheet_mut(0)
        .unwrap()
        .set_value(target("D2").address, 9.)
        .unwrap();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(target("D1").address, "PHI({0;1})")
        .unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    assert_eq!(
        direct.cell(target("D1")).unwrap(),
        CellValue::Error(CellError::Spill)
    );
}
#[test]
fn right_tails_and_large_chi_square_transitions_remain_representable() {
    let b = book();
    near(call(&b, "F.DIST.RT(1E20,2,2)"), 1e-20);
    near(
        call(&b, "CHISQ.DIST.RT(10000000000,10000000000)"),
        0.499_998_119_368_054_8,
    );
    let CellValue::Number(value) = call(&b, "GAUSS(1E-20)") else {
        panic!()
    };
    assert!(value > 0.);
}
#[test]
fn beta_scaled_endpoint_identity_survives_an_underflowing_distance() {
    let b = book();
    let CellValue::Number(actual) =
        call(&b, "BETA.DIST(0.9999999999999999,1,1E-300,TRUE,-1E308,1)")
    else {
        panic!()
    };
    // Beta(1,b) CDF = -expm1(b*log((B-x)/(B-A))), independently rounded at100 digits.
    let expected = 7.459330092118432e-298;
    assert!(
        actual > 0.0 && (actual - expected).abs() <= expected * 3e-12,
        "{actual:e}"
    );
}

#[test]
fn tiny_beta_lower_tail_uses_its_positive_defining_integral() {
    let b = book();
    let CellValue::Number(actual) = call(&b, "BETA.DIST(0.9,0.5,1E-300,TRUE)") else {
        panic!()
    };
    // t=u^2 makes the shape-one-half, tiny-other-shape leading integral 2*atanh(sqrt(x)).
    let expected = 2e-300 * 0.9f64.sqrt().atanh();
    assert!(
        actual > 0.0 && (actual - expected).abs() <= expected * 3e-12,
        "{actual:e}"
    );
}

#[test]
fn concentrated_small_shape_beta_cdf_saturates_only_with_a_tail_bound() {
    let b = book();
    near(call(&b, "BETA.DIST(0.5,0.01,1E8,TRUE)"), 1.);
    near(call(&b, "BETA.DIST(0.9,0.01,1E8,TRUE)"), 1.);
}

#[test]
fn typed_distribution_parameters_preserve_logical_and_prior_defaults() {
    let b = book();
    // Bernoulli mass at zero is 1-p; omission of the upper bound repeats the lower.
    near(call(&b, "BINOM.DIST.RANGE(1,0.25,0,)"), 0.75);
    // The uniform CDF is x even when its logical selector is supplied as text.
    near(call(&b, "BETA.DIST(0.25,1,1,\"TRUE\",,)"), 0.25);
    assert_eq!(
        call(&b, "BETA.DIST(1/0,1,1,#N/A)"),
        CellValue::Error(CellError::Div0)
    );
}
