//! Regression contracts derived from the exact systems in REGRESSION_SOURCES.md.
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, Workbook};
fn book() -> Workbook {
    let mut b = Workbook::new();
    b.add_sheet("S").unwrap();
    b
}
fn target(cell: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: cell.parse().unwrap(),
    }
}
fn call(b: &Workbook, formula: &str) -> CellValue {
    match run_formula(
        b,
        0,
        target("Z100").address,
        formula,
        Default::default(),
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn near(v: CellValue, e: f64) {
    let CellValue::Number(a) = v else {
        panic!("{v:?}")
    };
    assert!((a - e).abs() <= e.abs().max(1.) * 1e-10, "{a} != {e}");
}
#[test]
fn rational_statistics_are_independent_error_bearing_cells() {
    let b = book();
    let l = 2f64.ln();
    for (r, c, e) in [
        (2, 1, l / (12f64).sqrt()),
        (2, 2, l * 5f64.sqrt() / 6.),
        (3, 1, 0.75),
        (3, 2, l / (6f64).sqrt()),
        (4, 1, 3.),
        (4, 2, 1.),
        (5, 1, l * l / 2.),
        (5, 2, l * l / 6.),
    ] {
        near(
            call(
                &b,
                &format!("INDEX(LOGEST({{1;2;2}},{{0;1;2}},TRUE,TRUE),{r},{c})"),
            ),
            e,
        );
    }
    for (r, c, e) in [
        (1, 1, 0.5),
        (1, 2, 7. / 6.),
        (2, 1, (1f64 / 12.).sqrt()),
        (2, 2, 5f64.sqrt() / 6.),
        (3, 1, 0.75),
        (3, 2, (1f64 / 6.).sqrt()),
        (4, 1, 3.),
        (4, 2, 1.),
        (5, 1, 0.5),
        (5, 2, 1. / 6.),
    ] {
        near(
            call(
                &b,
                &format!("INDEX(LINEST({{1;2;2}},{{0;1;2}},TRUE,TRUE),{r},{c})"),
            ),
            e,
        );
    }
    near(call(&b, "INDEX(LINEST({1;1},{1;2},FALSE,TRUE),3,1)"), 0.9);
    assert_eq!(
        call(&b, "INDEX(LINEST({1;1},{1;2},FALSE,TRUE),2,2)"),
        CellValue::Error(CellError::NA)
    );
    assert_eq!(
        call(&b, "INDEX(LINEST({1;3;5},{0,0;1,0;2,0},TRUE,TRUE),3,3)"),
        CellValue::Error(CellError::NA)
    );
}
#[test]
fn regression_orientation_defaults_and_exponential_predictions() {
    let b = book();
    for (f, e) in [
        ("LINEST({1;3;4;6},{0,0;1,0;0,1;1,1})", 3.),
        ("INDEX(LINEST({1,3,4,6},{0,1,0,1;0,0,1,1}),1,2)", 2.),
        ("INDEX(LINEST({1;3;4;6},{0,0;1,0;0,1;1,1}),1,3)", 1.),
        ("TREND({1,3,4,6},{0,1,0,1;0,0,1,1},{2,4;3,5})", 14.),
        (
            "INDEX(TREND({1,3,4,6},{0,1,0,1;0,0,1,1},{2,4;3,5}),1,2)",
            24.,
        ),
        ("LOGEST({3;6;12},{0;1;2})", 2.),
        ("INDEX(LOGEST({3;6;12},{0;1;2}),1,2)", 3.),
        ("GROWTH({3;6;12},{0;1;2},{4})", 48.),
        ("GROWTH({2;4;8},,{4},FALSE)", 16.),
        ("LINEST({3;5;7})", 2.),
        ("INDEX(TREND({3;5;7}),3,1)", 7.),
        ("INDEX(LINEST({1;3;5},{0,0;1,1;2,2}),1,1)", 0.),
        ("INDEX(LOGEST({1;2;4},{0,0;1,1;2,2}),1,1)", 1.),
        ("INDEX(LOGEST({2;4;8},,FALSE),1,2)", 1.),
    ] {
        near(call(&b, f), e);
    }
}
#[test]
fn errors_keep_original_geometry_and_optional_arguments() {
    let b = book();
    for (f, e) in [
        ("LINEST({1;2},{1,2})", CellError::Ref),
        ("GROWTH({0;1})", CellError::Num),
        ("LOGEST({-1;2})", CellError::Num),
        ("LINEST({1;\"x\";3})", CellError::Value),
        ("LINEST({1;#N/A;3})", CellError::NA),
        ("LINEST({1;2},0)", CellError::Ref),
        ("GROWTH({1;2},{0;1},{0},#N/A)", CellError::NA),
    ] {
        assert_eq!(call(&b, f), CellValue::Error(e), "{f}");
    }
    near(call(&b, "INDEX(LINEST({1;3;5},,,),1,2)"), -1.);
    near(call(&b, "INDEX(LINEST({1;3;5},,FALSE),1,2)"), 0.);
}
#[test]
fn direct_overlay_edit_and_current_snapshot_preserve_spills() {
    let mut b = book();
    for (c, f) in [
        ("D1", "LINEST({1;2;2},{0;1;2},TRUE,TRUE)"),
        ("H1", "LOGEST({3;6;12},{0;1;2})"),
        ("L1", "GROWTH({3;6;12},{0;1;2},{3;4})"),
        ("P1", "LINEST({1;2;2},A1:A3)"),
    ] {
        b.sheet_mut(0)
            .unwrap()
            .set_formula(target(c).address, f)
            .unwrap();
    }
    for (c, v) in [("A1", 0.), ("A2", 1.), ("A3", 2.)] {
        b.sheet_mut(0)
            .unwrap()
            .set_value(target(c).address, v)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    for (c, e) in [
        ("D1", 0.5),
        ("E1", 7. / 6.),
        ("D3", 0.75),
        ("E5", 1. / 6.),
        ("H1", 2.),
        ("I1", 3.),
        ("L2", 48.),
        ("P1", 0.5),
    ] {
        near(direct.cell(target(c)).unwrap(), e);
        near(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(c))
                .unwrap(),
            e,
        );
    }
    overlay.set_value(target("A3"), 4.).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("P1"))
            .unwrap(),
        5. / 26.,
    );
    let (source, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&source).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("L2"))
            .unwrap(),
        48.,
    );
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("P1"))
            .unwrap(),
        5. / 26.,
    );
}
#[test]
fn lazy_branches_pinned_profile_and_operational_budget_are_distinct() {
    let b = book();
    near(call(&b, "IF(FALSE,GROWTH({0;1}),7)"), 7.);
    let pinned = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    for f in ["LINEST({1;2})", "LOGEST({1;2})", "GROWTH({1;2})"] {
        assert!(matches!(
            run_formula_with_configuration(
                &b,
                0,
                target("Z100").address,
                f,
                &pinned,
                Default::default()
            ),
            EvaluationOutcome::Complete {
                value: CellValue::Error(CellError::Name),
                ..
            }
        ));
    }
    assert!(matches!(
        run_formula(
            &b,
            0,
            target("Z100").address,
            "IFERROR(LINEST({1;2;3;4},{0,0;1,0;0,1;1,1},TRUE,TRUE),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 30,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn trend_admission_remains_distinct_from_generic_rank_zero_fits() {
    let b = book();
    near(call(&b, "TREND({5;5},{0;1E-300},1E308)"), 5.);
    near(call(&b, "TREND({0;1E-300},{0;1E-300},1E308)"), 1E308);
    near(call(&b, "GROWTH({5;5},{0;1E-300},1E308)"), 5.);
    assert_eq!(
        call(&b, "TREND({3},{1},{3},TRUE)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        call(&b, "TREND({3;5},{0;0},{3},FALSE)"),
        CellValue::Error(CellError::Value)
    );
    near(call(&b, "INDEX(LINEST({3},{1}),1,2)"), 3.);
    near(call(&b, "LINEST({3;5},{0;0},FALSE)"), 0.);
    near(call(&b, "FORECAST.LINEAR(3,{1;2;2},{0;1;2})"), 8. / 3.);
    near(call(&b, "TREND({1;2;2},{0;1;2},{3})"), 8. / 3.);
}

#[test]
fn options_from_references_stats_cost_and_blocked_spill_recovery() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(target("A1").address, true)
        .unwrap();
    b.sheet_mut(0)
        .unwrap()
        .set_formula(target("D1").address, "LINEST({1;2;2},{0;1;2},A1,A1)")
        .unwrap();
    b.sheet_mut(0)
        .unwrap()
        .set_value(target("E1").address, 99.)
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    overlay.set_value(target("E1"), CellValue::Blank).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D3"))
            .unwrap(),
        0.75,
    );
    let run = |stats| {
        run_formula(
            &b,
            0,
            target("Z100").address,
            &format!("LINEST({{1;2;2}},{{0;1;2}},TRUE,{stats})"),
            Default::default(),
            Default::default(),
        )
    };
    let plain = run("FALSE");
    let stats = run("TRUE");
    assert!(stats.usage().work_units > plain.usage().work_units);
}
