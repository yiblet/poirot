//! Legacy contracts checked with elementary probability equations and counts.
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
        other => panic!("{f}: {other:?}"),
    }
}
fn near(v: CellValue, e: f64) {
    let CellValue::Number(a) = v else {
        panic!("{v:?}")
    };
    assert!((a - e).abs() < e.abs().max(1.) * 3e-11, "{a} != {e}");
}
fn cases() -> [(&'static str, f64); 23] {
    // Uniform beta, binomial counting, exponential/gamma shape 1, F(2,2)
    // survival 1/(1+x), Cauchy two-tail median, and equal-sample tests.
    [
        ("BETADIST(3,1,1,2,6)", 0.25),
        ("BETAINV(0.25,1,1,2,6)", 3.),
        ("BINOMDIST(1,4,0.5,FALSE)", 0.25),
        ("CHIDIST(2*LN(2),2)", 0.5),
        ("CHIINV(0.5,2)", 2. * 2f64.ln()),
        ("CHITEST({2,2;2,2},{2,2;2,2})", 1.),
        ("CONFIDENCE(0.05,1,1)", 1.9599639845400542),
        ("CRITBINOM(4,0.5,5/16)", 1.),
        ("EXPONDIST(2,0.5,TRUE)", 1. - (-1f64).exp()),
        ("FDIST(3,2,2)", 0.25),
        ("FINV(0.25,2,2)", 3.),
        ("FTEST({0;1;2},{2;3;4})", 1.),
        ("GAMMADIST(2,1,2,TRUE)", 1. - (-1f64).exp()),
        ("GAMMAINV(0.5,1,2)", 2. * 2f64.ln()),
        ("HYPGEOMDIST(1,2,2,4)", 2. / 3.),
        ("LOGINV(0.5,0,1)", 1.),
        ("LOGNORMDIST(1,0,1)", 0.5),
        ("NEGBINOMDIST(1,2,0.5)", 0.25),
        ("QUARTILE({0;4},1)", 1.),
        ("TINV(0.5,1)", 1.),
        ("TTEST({0;1;2},{0;1;2},2,2)", 1.),
        ("WEIBULL(2,1,2,TRUE)", 1. - (-1f64).exp()),
        ("ZTEST({0;1;2},1)", 0.5),
    ]
}
#[test]
fn all_23_execute_independent_controls_in_direct_and_overlay() {
    let mut b = book();
    for (i, (f, e)) in cases().into_iter().enumerate() {
        near(call(&b, f), e);
        b.sheet_mut(0)
            .unwrap()
            .set_formula(target(&format!("A{}", i + 1)).address, f)
            .unwrap();
    }
    let mut direct = Evaluation::with_workbook(&b)
        .build(EvaluationContext::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    for (i, (_, e)) in cases().into_iter().enumerate() {
        let t = target(&format!("A{}", i + 1));
        near(direct.cell(t).unwrap(), e);
        near(
            overlay
                .evaluation(&EvaluationContext::default())
                .unwrap()
                .cell(t)
                .unwrap(),
            e,
        );
    }
}
#[test]
fn bounds_fixed_selectors_required_selectors_and_degree_domains() {
    let b = book();
    for (f, e) in [
        ("BETADIST(0.25,1,1)", 0.25),
        ("BETADIST(0.25,1,1,,)", 0.25),
        ("BETAINV(1,1,1,2,6)", 6.),
        ("LOGNORMDIST(1,0,1)", 0.5),
        ("HYPGEOMDIST(1.9,2.9,2.9,4.9)", 2. / 3.),
        ("NEGBINOMDIST(1.9,2.9,0.5)", 0.25),
        ("QUARTILE({0;4},0)", 0.),
        ("QUARTILE({0;4},2.9)", 2.),
        ("QUARTILE({0;4},4)", 4.),
        ("TINV(1,1.9)", 0.),
        ("FINV(1,2,2)", 0.),
        ("GAMMAINV(0,1,2)", 0.),
        ("CHIDIST(0,10000000000.9)", 1.),
        ("CRITBINOM(0,0.5,0.5)", 0.),
        ("ZTEST({0;1;2},1,1)", 0.5),
    ] {
        near(call(&b, f), e);
    }
    for f in [
        "BETADIST(3,1,1,4)",
        "BETAINV(0,1,1)",
        "FDIST(1,10000000000,2)",
        "FDIST(1,2,10000000000)",
        "FINV(1,10000000000,2)",
        "FINV(1,2,10000000000)",
        "CHIDIST(0,10000000001)",
        "HYPGEOMDIST(0,0,1,4)",
        "HYPGEOMDIST(0,1,0,4)",
        "CRITBINOM(4,0,0.5)",
        "CRITBINOM(4,1,0.5)",
        "CRITBINOM(4,0.5,0)",
        "CRITBINOM(4,0.5,1)",
        "QUARTILE({0;4},5)",
        "QUARTILE({\"x\"},1)",
        "TINV(0,1)",
        // Retained finite-quantile policy; these are not native Excel observations.
        "CHIINV(0,2)",
        "FINV(0,2,2)",
        "GAMMAINV(1,1,2)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Num), "{f}");
    }
    for f in [
        "BINOMDIST(1,4,0.5)",
        "BINOMDIST(1/0,4,0.5)",
        "EXPONDIST(2,1)",
        "GAMMADIST(1,1,1)",
        "WEIBULL(1,1,1)",
        "LOGNORMDIST(1,0,1,TRUE)",
        "NEGBINOMDIST(1,2,0.5,TRUE)",
        "HYPGEOMDIST(1,2,2,4,TRUE)",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Value), "{f}");
    }
}
#[test]
fn origin_error_precedence_and_collection_policies_remain_explicit() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (c, v) in [
        ("A1", CellValue::from("0.5")),
        ("A2", 0.5.into()),
        (
            "A3",
            CellValue::TextUtf16(poirot::model::ExcelText::from("0.5")),
        ),
        ("B1", true.into()),
        ("B2", "ignored".into()),
    ] {
        s.set_value(target(c).address, v).unwrap();
    }
    near(call(&b, "BETAINV(A1,1,1)"), 0.5);
    near(call(&b, "BETADIST(A2,1,1)"), 0.5);
    near(call(&b, "GAMMAINV(A2,1,2)"), 2. * 2f64.ln());
    for (f, e) in [
        ("QUARTILE(B1:B2,2)", CellError::Num),
        ("CHITEST({1;2},{1;2;3})", CellError::NA),
        ("CHITEST({1},{1})", CellError::NA),
        ("FTEST({1;1},{0;2})", CellError::Div0),
        ("TTEST({1;2},{1;2;3},2,1)", CellError::NA),
        ("ZTEST({true;\"x\"},0)", CellError::NA),
        ("BETADIST(0.5,1/0,1)", CellError::Div0),
    ] {
        assert_eq!(call(&b, f), CellValue::Error(e), "{f}");
    }
    let CellValue::Number(p) = call(&b, "ZTEST({0;1;2},2,1)") else {
        panic!()
    };
    assert!(p > 0.5 && p < 1.);
}
#[test]
fn lazy_pinned_and_operational_failures() {
    let b = book();
    near(call(&b, "IF(FALSE,FINV(0.3,2,10000000000),7)"), 7.);
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    for (f, _) in cases() {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &b,
                    0,
                    target("Z100").address,
                    f,
                    &config,
                    Default::default()
                ),
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Name),
                    ..
                }
            ),
            "{f}"
        );
    }
    for f in [
        "IFERROR(BETAINV(0.3,2000000000,1000000000),99)",
        "IFERROR(CHIINV(0.3,10000000000),99)",
        "IFERROR(TINV(0.3,1000000000),99)",
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
}
#[test]
fn scalar_slots_lift_without_flattening_sample_arguments() {
    let mut b = book();
    for (c, f) in [
        ("D1", "BETADIST({3;4},1,1,2,6)"),
        ("G1", "LOGNORMDIST({1;1},0,1)"),
        ("J1", "HYPGEOMDIST({0;1},2,2,4)"),
        ("M1", "NEGBINOMDIST({0;1},2,0.5)"),
        ("P1", "TTEST({0;1;2},{0;1;2},{1;2},2)"),
        ("S1", "ZTEST({0;1;2},{1;1},1)"),
    ] {
        b.sheet_mut(0)
            .unwrap()
            .set_formula(target(c).address, f)
            .unwrap();
    }
    let mut direct = Evaluation::with_workbook(&b)
        .build(EvaluationContext::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    for (c, e) in [
        ("D1", 0.25),
        ("D2", 0.5),
        ("G2", 0.5),
        ("J1", 1. / 6.),
        ("J2", 2. / 3.),
        ("M1", 0.25),
        ("M2", 0.25),
        ("P1", 0.5),
        ("P2", 1.),
        ("S2", 0.5),
    ] {
        near(direct.cell(target(c)).unwrap(), e);
        near(
            overlay
                .evaluation(&EvaluationContext::default())
                .unwrap()
                .cell(target(c))
                .unwrap(),
            e,
        );
    }
}
