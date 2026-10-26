//! Public modern-finance contracts from documented equations and independent examples.
use poirot::eval::{
    overlay::{Evaluation, EvaluationContext, Overlay},
    *,
};
use poirot::model::{CellError, CellValue, Workbook};
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
        (actual - expected).abs() <= 2e-12 * expected.abs() + 1e-12,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn all_fifteen_names_have_independent_answers() {
    let b = book();
    for (formula, expected) in [
        ("CUMIPMT(0.1,2,210,1,2,0)", -32.0),
        ("CUMPRINC(0.1,2,210,1,2,0)", -210.0),
        ("CUMIPMT(0.1,2,210,1,2,1)", -10.0),
        ("CUMPRINC(0.1,2,210,1,1,1)", -110.0),
        ("DB(1000,250,2,2,6)", 375.0),
        ("DDB(1000,100,5,2)", 240.0),
        ("EFFECT(0.2,2.9)", 0.21),
        ("FVSCHEDULE(100,{0.1;0;0.2})", 132.0),
        ("ISPMT(0.1,0,2,210)", -21.0),
        ("NOMINAL(0.21,2.9)", 0.2),
        ("PDURATION(0.1,100,121)", 2.0),
        ("RRI(2,100,121)", 0.1),
        ("SLN(1000,100,3)", 300.0),
        ("SYD(1000,100,3,2)", 300.0),
        ("VDB(1000,0,5,2.5,4.5)", 234.0),
        ("VDB(1E308,0,0.5,0,0.5)", 1e308),
        ("VDB(1000,0,5,2.5,4.5,,TRUE)", 184.32),
        ("XIRR({-100;110},{1000;1365})", 0.1),
        ("XNPV(0.1,{-100;110},{1000;1365})", 0.0),
        ("_xlfn.RRI(2,100,121)", 0.1),
        ("_xlfn.PDURATION(0.1,100,121)", 2.0),
        ("XIRR({-100;230;-132},{1000;1365;1730},0.08)", 0.1),
        ("XIRR({-100;230;-132},{1000;1365;1730},0.3)", 0.2),
        ("XIRR({-100;200;-100},{1000;1365;1730},0)", 0.0),
    ] {
        near(call(&b, formula), expected);
    }
}
#[test]
fn defaults_blank_coercion_and_domain_errors_are_explicit() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, "2").unwrap();
    near(call(&b, "EFFECT(0.2,A1)"), 0.21);
    near(call(&b, "DB(1000,250,2,1,)"), 500.0);
    near(call(&b, "DDB(1000,100,5,1,)"), 400.0);
    near(call(&b, "XIRR({-100;110},{1000;1365},)"), 0.1);
    near(call(&b, "FVSCHEDULE(100,A2:A3)"), 100.0);
    for f in [
        "EFFECT(0.2,A2)",
        "DB(1000,250,2,1,A2)",
        "DDB(1000,100,5,1,A2)",
        "CUMIPMT(0,2,210,1,2,0)",
        "CUMPRINC(0.1,2,210,1,3,0)",
        "CUMIPMT(0.1,2,210,1,2,2)",
        "XIRR({1;2},{1000;1365})",
        "XIRR({-100;50;-100},{1000;1365;1730})",
        "XNPV(0.1,{-100;110},{1000;999})",
        "XNPV(0.1,{-100;110},{1000})",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Num), "{f}");
    }
    for f in [
        "EFFECT()",
        "SLN(1/0,0)",
        "FVSCHEDULE(100,{-1;\"bad\"})",
        "FVSCHEDULE(100,{TRUE})",
        "XNPV(0.1,{-100;\"110\"},{1000;1365})",
        "XNPV(0.1,{-100;110},{1000;-1})",
    ] {
        assert_eq!(call(&b, f), CellValue::Error(CellError::Value), "{f}");
    }
    assert_eq!(
        call(&b, "FVSCHEDULE(100,{-1;#DIV/0!})"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(call(&b, "SLN(100,0,0)"), CellValue::Error(CellError::Div0));
    near(call(&b, "FVSCHEDULE(0,{1E308;1E308})"), 0.0);
}
#[test]
fn positional_dates_remain_paired_and_truncate_without_sorting() {
    let b = book();
    near(
        call(&b, "XNPV(1,{-100;80;40},{1000.9;1730.9;1365.9})"),
        -60.0,
    );
    near(call(&b, "XNPV(0,{-100,0,110},{1000;1200;1365})"), 10.0);
    // The blank value remains at day1200; the later110 must not move there.
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, -100.0).unwrap();
    s.set_value(at("A3").address, 110.0).unwrap();
    near(call(&b, "XNPV(0.1,A1:A3,{1000;1200;1365})"), 0.0);
    assert_eq!(
        call(&b, "XNPV(0.1,A1:A3,{1000;\"bad\";1365})"),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn selected_lifting_spill_children_and_overlay_value_date_edits() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, value) in [
        ("A1", -100.0),
        ("A2", 110.0),
        ("B1", 1000.0),
        ("B2", 1365.0),
    ] {
        s.set_value(at(cell).address, value).unwrap();
    }
    for (cell, formula) in [
        ("D1", "FVSCHEDULE({100;200},{0.1;0.2})"),
        ("G1", "XNPV({0;0.1},A1:A2,B1:B2)"),
        ("J1", "XIRR(A1:A2,B1:B2,{0.05;0.2})"),
        ("M1", "SLN({1000;2000},100,{3,5})"),
    ] {
        s.set_formula(at(cell).address, formula).unwrap();
    }
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    for (cell, expected) in [("D2", 264.0), ("G2", 0.0), ("J2", 0.1), ("N2", 380.0)] {
        near(direct.cell(at(cell)).unwrap(), expected);
    }
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.1,
    );
    overlay.set_value(at("A2"), 121.0).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.21,
    );
    overlay.set_value(at("B2"), 1730.0).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.1,
    );
    let (book, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&book).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.1,
    );
    restored.set_value(at("A2"), 144.0).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("J2"))
            .unwrap(),
        0.2,
    );
}
#[test]
fn schedule_reads_continue_after_zero_and_dependency_changes_recompute() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, -1.0).unwrap();
    s.set_value(at("A2").address, 0.1).unwrap();
    s.set_formula(at("D1").address, "FVSCHEDULE(100,A1:A2)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        0.0,
    );
    overlay.set_value(at("A2"), "bad").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        CellValue::Error(CellError::Value)
    );
}
#[test]
fn limits_are_operational_laziness_and_pinned_availability_are_preserved() {
    let b = book();
    {
        let f = "IFERROR(VDB(1000,0,200000,0,100001),99)";
        assert!(matches!(
            run_formula(
                &b,
                0,
                at("Z100").address,
                f,
                Default::default(),
                Default::default()
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(VDB(1000,0,1000,0,999),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(XIRR({-100;110},{1000;1365},0.5),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 1,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    near(call(&b, "IF(FALSE,VDB(1000,0,200000,0,100001),42)"), 42.0);
    let config = RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi);
    assert!(matches!(
        run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            "RRI(2,100,121)",
            &config,
            Default::default()
        ),
        EvaluationOutcome::Complete {
            value: CellValue::Error(CellError::Name),
            ..
        }
    ));
}

#[test]
fn every_lifted_collection_reduction_is_charged() {
    let b = book();
    for formula in [
        "IFERROR(FVSCHEDULE(SEQUENCE(20),SEQUENCE(20)/100),99)",
        "IFERROR(XNPV(SEQUENCE(20)/100,SEQUENCE(20),SEQUENCE(20)+1000),99)",
    ] {
        assert!(
            matches!(
                run_formula(
                    &b,
                    0,
                    at("Z100").address,
                    formula,
                    Default::default(),
                    EvaluationBudgets {
                        run_work: 300,
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
fn epoch_validation_spill_shrink_and_blockers_remain_host_owned() {
    for system in [
        poirot::model::DateSystem::Excel1900,
        poirot::model::DateSystem::Excel1904,
    ] {
        let mut b = book();
        b.date_system = system;
        near(call(&b, "XIRR({-100;110},{0;365})"), 0.1);
        near(call(&b, "XNPV(0,{-100;110},{59;60})"), 10.0);
        assert_eq!(
            call(&b, "XNPV(0,{-100;110},{1000;4000000})"),
            CellValue::Error(CellError::Value)
        );
    }
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_formula(at("D1").address, "FVSCHEDULE({100;200},{0.1;0.2})")
        .unwrap();
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        264.0,
    );
    overlay
        .set_formula(at("D1"), "FVSCHEDULE(100,{0.1;0.2})")
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(at("D2"), 7.0).unwrap();
    overlay
        .set_formula(at("D1"), "FVSCHEDULE({100;200},{0.1;0.2})")
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        7.0,
    );
}

#[test]
fn exact_zero_rate_equation_keeps_small_cash_flows_between_large_ones() {
    let b = book();
    near(call(&b, "XNPV(0,{-1E308;1;1E308},{1000;1365;1730})"), 1.0);
    near(call(&b, "CUMPRINC(.1,1E20,100,1,1E20,1)"), -100.0);
}
