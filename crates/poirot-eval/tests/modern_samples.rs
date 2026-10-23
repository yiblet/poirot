//! Public sample contracts from elementary independent mathematics (SAMPLE_SOURCES.md).
use poirot_eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot_model::{CellError, CellValue, Workbook};
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
fn near(value: CellValue, expected: f64) {
    let CellValue::Number(actual) = value else {
        panic!("{value:?}")
    };
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.) * 3e-12,
        "{actual:e} != {expected:e}"
    );
}
fn cases() -> [(&'static str, f64); 21] {
    [
        ("CHISQ.TEST({3,1;1,3},{2,2;2,2})", 0.15729920705028513),
        ("F.TEST({0;1;2},{0;2;4})", 0.4),
        ("FISHER(0.5)", 3f64.ln() * 0.5),
        ("FISHERINV(0)", 0.),
        ("HARMEAN(2,6)", 3.),
        ("KURT(-1,-1,1,1)", -6.),
        ("MODE.MULT({7;2;7;2})", 7.),
        ("MODE.SNGL({7;2;7;2})", 7.),
        ("PERCENTILE.EXC({0;10;20},3/8)", 5.),
        ("PERMUT(5,3)", 60.),
        ("PERMUTATIONA(2,4)", 16.),
        ("PROB({0;1;2},{0.25;0.25;0.5},1,2)", 0.75),
        ("QUARTILE.EXC({0;10;20},2)", 10.),
        ("RANK.AVG(2,{2;2;7})", 2.5),
        ("RSQ({1;0;1},{0;1;2})", 0.),
        ("SKEW(-1,0,1)", 0.),
        ("SKEW.P(-1,0,1)", 0.),
        ("STEYX({1;0;1},{0;1;2})", (2f64 / 3.).sqrt()),
        ("T.TEST({0;2},{2;3;4},2,3)", 0.2481885380840121),
        ("TRIMMEAN({-100;1;2;3;100},0.4)", 2.),
        ("Z.TEST({0;1;2},1)", 0.5),
    ]
}
#[test]
fn all_twenty_one_names_execute_direct_overlay_and_current_snapshot() {
    let mut b = book();
    for (i, (formula, expected)) in cases().iter().enumerate() {
        near(call(&b, formula), *expected);
        b.sheet_mut(0)
            .unwrap()
            .set_formula(
                poirot_model::CellAddress {
                    row: 0,
                    col: i as u32 * 2,
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
    for (i, (_, expected)) in cases().iter().enumerate() {
        let t = EvaluationTarget {
            sheet: 0,
            address: poirot_model::CellAddress {
                row: 0,
                col: i as u32 * 2,
            },
        };
        near(direct.cell(t).unwrap(), *expected);
        near(
            overlay.evaluation(&context).unwrap().cell(t).unwrap(),
            *expected,
        );
    }
    near(direct.cell(target("M2")).unwrap(), 2.);
    let (source, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&source).unwrap();
    for (i, (_, expected)) in cases().iter().enumerate() {
        let t = EvaluationTarget {
            sheet: 0,
            address: poirot_model::CellAddress {
                row: 0,
                col: i as u32 * 2,
            },
        };
        near(
            restored.evaluation(&context).unwrap().cell(t).unwrap(),
            *expected,
        );
    }
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(target("M2"))
            .unwrap(),
        2.,
    );
}
#[test]
fn scalar_slots_lift_while_collections_reduce() {
    let mut b = book();
    for (cell, f) in [
        ("A1", "PERCENTILE.EXC({0;10;20},{0.375;0.5})"),
        ("D1", "RANK.AVG({2;7},{2;2;7},{0,1})"),
        ("H1", "T.TEST({0;2},{0;1;2},{1;2},{2,3})"),
        ("K1", "PROB({0;1;2},{0.25;0.25;0.5},{0;1},{1,2})"),
        ("N1", "Z.TEST({0;1;2},{1;2},{1,2})"),
        ("Q1", "QUARTILE.EXC({0;10;20},{1;2;3})"),
        ("T1", "TRIMMEAN({-100;1;2;3;100},{0;0.4})"),
        ("W1", "PERMUT({4;5},{0,2})"),
        ("Z1", "FISHERINV({0;1000})"),
        ("AC1", "FISHER({0;0.5})"),
        ("AF1", "PERMUTATIONA({2;3},{0,2})"),
    ] {
        b.sheet_mut(0)
            .unwrap()
            .set_formula(target(cell).address, f)
            .unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    for (cell, e) in [
        ("A1", 5.),
        ("A2", 10.),
        ("D1", 2.5),
        ("E1", 1.5),
        ("D2", 1.),
        ("E2", 3.),
        ("H1", 0.5),
        ("I2", 1.),
        ("K1", 0.5),
        ("L2", 0.75),
        ("N1", 0.5),
        ("O1", 0.5),
        ("Q1", 0.),
        ("Q3", 20.),
        ("T1", 1.2),
        ("T2", 2.),
        ("X2", 20.),
        ("Z2", 1.),
        ("AC2", 0.5 * 3f64.ln()),
        ("AG2", 9.),
    ] {
        near(direct.cell(target(cell)).unwrap(), e);
        near(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(cell))
                .unwrap(),
            e,
        );
    }
}
#[test]
fn origins_errors_geometry_and_exact_ranks() {
    let mut b = book();
    for (cell, value) in [
        ("A1", CellValue::Text("2".into())),
        ("A2", true.into()),
        ("A3", 2f64.into()),
        ("A4", 6f64.into()),
    ] {
        b.sheet_mut(0)
            .unwrap()
            .set_value(target(cell).address, value)
            .unwrap();
    }
    near(call(&b, "HARMEAN(A1:A4)"), 3.);
    near(call(&b, "HARMEAN(\"2\",6)"), 3.);
    near(call(&b, "MODE.SNGL({TRUE;\"x\";2;2})"), 2.);
    near(call(&b, "MODE.SNGL(\"2\",2)"), 2.);
    near(call(&b, "HARMEAN(TRUE,3)"), 1.5);
    near(call(&b, "PROB({0;1},{0.25;0.75})"), 0.25);
    near(call(&b, "PROB({0;1},{0.25;0.75},1)"), 0.75);
    near(call(&b, "Z.TEST({0;1;2},1,)"), 0.5);
    near(call(&b, "RANK.AVG(2,{2;2;7},-1)"), 1.5);
    near(call(&b, "RANK.AVG(2,{2;2;7},3)"), 1.5);
    // Asymmetric exclusions must remove entire pairs, never filter independently.
    near(call(&b, "RSQ({1;\"x\";5;7},{0;99;2;3})"), 1.);
    near(call(&b, "STEYX({1;\"x\";5;7},{0;99;2;3})"), 0.);
    near(
        call(&b, "T.TEST({0;\"x\";2;4},{1;88;4;7},2,1)"),
        0.07417990022744854,
    );
    for (f, e) in [
        ("CHISQ.TEST({1,2},{1;2})", CellError::NA),
        ("CHISQ.TEST(1,1)", CellError::NA),
        ("CHISQ.TEST({1;2},{0;2})", CellError::Div0),
        ("F.TEST({1;1},{1;2})", CellError::Div0),
        ("FISHER(1)", CellError::Num),
        ("FISHERINV(\"x\")", CellError::Value),
        ("HARMEAN(0,2)", CellError::Num),
        ("KURT(1,2,3)", CellError::Div0),
        ("SKEW.P(1,2)", CellError::Div0),
        ("SKEW(1,1,1)", CellError::Div0),
        ("MODE.SNGL({1;2})", CellError::NA),
        ("MODE.MULT({1;2})", CellError::NA),
        ("PERCENTILE.EXC({0;10;20},0.2)", CellError::Num),
        ("QUARTILE.EXC({0;10;20},4)", CellError::Num),
        ("PERMUT(0,0)", CellError::Num),
        ("PERMUT(3,4)", CellError::Num),
        ("PERMUTATIONA(-1,2)", CellError::Num),
        ("PROB({1;2},{0;1},1,2)", CellError::Num),
        ("PROB({1;2},{0.5;0.25},1,2)", CellError::Num),
        ("PROB({1;2},{1},1,2)", CellError::NA),
        ("PROB({1;\"x\"},{0.5;0.5},1,2)", CellError::Value),
        ("RANK.AVG(3,{1;2})", CellError::NA),
        ("RSQ({1;2},{1})", CellError::NA),
        ("STEYX({1;2},{1;2})", CellError::Div0),
        ("T.TEST({1;2},{1;2},3,1)", CellError::Num),
        ("T.TEST({1;2},{1;2},2,4)", CellError::Num),
        ("T.TEST({1;2},{1;2;3},2,1)", CellError::NA),
        ("TRIMMEAN({1;2},1)", CellError::Div0),
        ("Z.TEST({1;2},0,0)", CellError::Div0),
        ("Z.TEST({1;2},0,A5)", CellError::Div0),
        ("HARMEAN(1/0,0)", CellError::Div0),
        ("T.TEST(1/0)", CellError::Value),
    ] {
        assert_eq!(call(&b, f), CellValue::Error(e), "{f}");
    }
}
#[test]
fn mode_spills_edit_shrink_expand_block_and_grouping_aliases() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, value) in [("A1", 7.), ("A2", 2.), ("A3", 7.), ("A4", 2.)] {
        s.set_value(target(cell).address, value).unwrap();
    }
    s.set_formula(target("D1").address, "MODE.MULT(A1:A4)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D2"))
            .unwrap(),
        2.,
    );
    overlay.set_value(target("A4"), 7.).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        7.,
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(target("D2"), 99.).unwrap();
    overlay.set_value(target("A4"), 2.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("D1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    for alias in ["MODE", "MODE.SNGL"] {
        near(
            call(
                &b,
                &format!("INDEX(GROUPBY({{1;1;1;1}},{{7;2;7;2}},{alias},0,0),1,2)"),
            ),
            7.,
        );
        near(
            call(
                &b,
                &format!("INDEX(GROUPBY({{1;1;1;1}},{{TRUE;\"x\";2;2}},{alias},0,0),1,2)"),
            ),
            2.,
        );
    }
    // New descriptor does not inherit pinned MODE's thirty-argument maximum.
    near(
        call(&b, &format!("MODE.SNGL({})", vec!["2"; 31].join(","))),
        2.,
    );
}
#[test]
fn scaled_public_controls_and_signed_z_tail() {
    let b = book();
    near(
        call(
            &b,
            "CHISQ.TEST({1.7976931348623157E308;1.7976931348623157E308},{1;1})",
        ),
        0.,
    );
    near(
        call(&b, "Z.TEST({-1E200;1E200},1E-200,1E-200)"),
        0.9213503964748574,
    );
    near(call(&b, "KURT(-1E200,-1E200,1E200,1E200)"), -6.);
    near(
        call(&b, "RSQ({1E200;2E200;3E200},{1E-200;2E-200;3E-200})"),
        1.,
    );
    near(call(&b, "Z.TEST({0;1;2},0,1)"), 0.0416322583317752);
    near(call(&b, "Z.TEST({0;1;2},2,1)"), 1. - 0.0416322583317752);
    near(
        call(&b, "T.TEST({0;2},{2;3;4},1.9,3.9)"),
        0.12409426904200605,
    );
    near(
        call(&b, "CHISQ.TEST({3;1;1;3},{2;2;2;2})"),
        0.5724067044708798,
    );
}
#[test]
fn laziness_profile_isolation_and_uncatchable_work() {
    let b = book();
    near(call(&b, "IF(FALSE,T.TEST({0;2},{2;3;4},2,3),7)"), 7.);
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    for (formula, _) in cases() {
        assert!(
            matches!(
                run_formula_with_configuration(
                    &b,
                    0,
                    target("Z100").address,
                    formula,
                    &config,
                    Default::default()
                ),
                EvaluationOutcome::Complete {
                    value: CellValue::Error(CellError::Name),
                    ..
                }
            ),
            "{formula}"
        );
    }
    for formula in [
        "IFERROR(T.TEST({0;2},{2;3;4},2,3),99)",
        "IFERROR(MODE.MULT({7;2;7;2}),99)",
        "IFERROR(PERMUT(200,100),99)",
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
fn every_lifted_sample_test_pays_for_its_tail_work() {
    let b = book();
    let single = run_formula(
        &b,
        0,
        target("Z100").address,
        "T.TEST({0;2},{2;3;4},2,3)",
        Default::default(),
        Default::default(),
    );
    let lifted = run_formula(
        &b,
        0,
        target("Z100").address,
        "T.TEST({0;2},{2;3;4},{2;2},3)",
        Default::default(),
        Default::default(),
    );
    assert!(matches!(single, EvaluationOutcome::Complete { .. }));
    assert!(matches!(lifted, EvaluationOutcome::Complete { .. }));
    assert!(lifted.usage().work_units > single.usage().work_units);
    assert!(matches!(
        run_formula(
            &b,
            0,
            target("Z100").address,
            "IFERROR(T.TEST({0;2},{2;3;4},{2;2},3),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: single.usage().work_units,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
