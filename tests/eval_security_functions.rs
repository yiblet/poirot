//! Public security contracts derived from documented equations and independent calendars.
use poirot::eval::overlay::{Evaluation, Overlay};
use poirot::eval::*;
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
        (actual - expected).abs() <= expected.abs() * 3e-12 + 1e-12,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn all_twenty_two_names_have_independent_mathematical_controls() {
    let b = book();
    let s = "DATE(2024,1,15)";
    let m = "DATE(2026,1,15)";
    let half = "DATE(2024,7,15)";
    for (formula, expected) in [
        (
            format!("ACCRINT({s},{half},DATE(2024,4,15),0.1,1000,2)"),
            25.,
        ),
        (format!("ACCRINTM({s},DATE(2024,4,15),0.1,1000)"), 25.),
        (format!("COUPDAYBS({s},{m},1)"), 0.),
        (format!("COUPDAYS({s},{m},1)"), 360.),
        (format!("COUPDAYSNC({s},{m},1)"), 360.),
        (format!("COUPNCD({s},{m},1)-DATE(2025,1,15)"), 0.),
        (format!("COUPNUM({s},{m},1)"), 2.),
        (format!("COUPPCD({s},{m},1)-{s}"), 0.),
        (format!("DISC({s},{half},95,100)"), 0.1),
        (format!("DURATION({s},{m},0.1,0.1,1)"), 21. / 11.),
        (format!("INTRATE({s},{half},95,100)"), 2. / 19.),
        (format!("MDURATION({s},{m},0.1,0.1,1)"), 210. / 121.),
        (format!("PRICE({s},{m},0.1,0.1,100,1)"), 100.),
        (format!("PRICEDISC({s},{half},0.1,100)"), 95.),
        (
            format!("PRICEMAT(DATE(2024,4,15),{half},{s},0.1,0.05)"),
            105. / 1.0125 - 2.5,
        ),
        (format!("RECEIVED({s},{half},95,0.1)"), 100.),
        (format!("TBILLEQ({s},DATE(2024,7,13),0.1)"), 36.5 / 342.),
        (format!("TBILLPRICE({s},DATE(2024,7,13),0.1)"), 95.),
        (format!("TBILLYIELD({s},DATE(2024,7,13),95)"), 2. / 19.),
        (format!("YIELD({s},{m},0.1,100,100,1)"), 0.1),
        (format!("YIELDDISC({s},{half},95,100)"), 2. / 19.),
        (
            format!("YIELDMAT(DATE(2024,4,15),{half},{s},0.1,105/1.0125-2.5)"),
            0.05,
        ),
    ] {
        near(call(&b, &formula), expected);
    }
}
#[test]
fn defaults_omissions_blank_and_error_channels_are_distinct() {
    let b = book();
    near(
        call(&b, "ACCRINTM(DATE(2024,1,15),DATE(2024,4,15),0.1)"),
        25.,
    );
    near(
        call(
            &b,
            "ACCRINT(DATE(2024,1,15),DATE(2024,7,15),DATE(2024,4,15),0.1,,2,,)",
        ),
        25.,
    );
    near(call(&b, "COUPDAYS(45000.8,46000.9,2.9,0.9)"), 180.);
    for (formula, error) in [
        ("ACCRINTM(45000,45090,0.1,A1)", CellError::Num),
        ("COUPDAYS(45000,46000,A1)", CellError::Num),
        ("COUPDAYS(45000,46000,,1)", CellError::Value),
        ("COUPDAYS(46000,45000,2)", CellError::Num),
        ("COUPDAYS(45000,46000,3)", CellError::Num),
        ("COUPDAYS(45000,46000,2,5)", CellError::Num),
        ("PRICE(45000,46000,-0.1,0,100,2)", CellError::Num),
        ("COUPDAYS(1/0,46000,2)", CellError::Div0),
        ("COUPDAYS(45000,4000000,2)", CellError::Value),
        ("COUPDAYS(\"not a date\",46000,2)", CellError::Value),
        ("TBILLYIELD(45000,45000,95)", CellError::Num),
        ("RECEIVED(45000,45180,100,2,2)", CellError::Num),
        ("COUPDAYS(1/0)", CellError::Value),
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(error), "{formula}");
    }
}
#[test]
fn direct_and_overlay_share_lifting_spill_and_invalidation() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 0.1).unwrap();
    s.set_formula(at("B1").address, "DATE(2026,1,15)").unwrap();
    s.set_formula(
        at("D1").address,
        "PRICE(DATE(2024,1,15),B1,{1;2}*A1,0,100,1)",
    )
    .unwrap();
    s.set_formula(at("G1").address, "COUPNUM(DATE(2024,1,15),B1,{1,2})")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    for (cell, value) in [("D2", 140.), ("D1", 120.), ("H1", 4.)] {
        near(direct.cell(at(cell)).unwrap(), value);
    }
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        140.,
    );
    overlay.set_value(at("A1"), 0.2).unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        180.,
    );
    overlay.set_formula(at("B1"), "DATE(2027,1,15)").unwrap();
    near(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        220.,
    );
    let (b, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut restored = state.attach(&b).unwrap();
    near(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        220.,
    );
    restored
        .set_formula(
            at("D1"),
            "PRICE(DATE(2024,1,15),DATE(2026,1,15),0.1,0,100,1)",
        )
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
fn worksheet_epoch_serials_and_leap_slot_are_preserved() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let mut b = book();
        b.date_system = system;
        near(
            call(
                &b,
                "COUPNCD(DATE(2024,1,15),DATE(2026,1,15),1)-DATE(2025,1,15)",
            ),
            0.,
        );
        near(call(&b, "ACCRINTM(59,60,0.1,3600,2)"), 1.);
    }
    let b = book();
    near(call(&b, "COUPPCD(1,366,1)"), 0.); // preceding1899-12-31 boundary remains representable as a serial.
}
#[test]
fn lazy_branches_and_spill_collisions_remain_host_owned() {
    let mut b = book();

    near(
        call(&b, "IF(FALSE,YIELD(45000,46000,0.1,1E-300,100,2),7)"),
        7.,
    );
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("D2").address, 9.).unwrap();
    s.set_formula(at("D1").address, "COUPDAYS({45000;45001},46000,2)")
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
fn bounded_bond_yield_work_cannot_be_caught_by_iferror() {
    let b = book();
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(YIELD(DATE(2024,1,15),DATE(2026,1,15),0,4,100,1),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
    near(
        call(&b, "YIELD(DATE(2024,1,15),DATE(2026,1,15),0,4,100,1)"),
        4.,
    );
    near(
        call(&b, "PRICE(DATE(2024,1,15),DATE(9999,1,15),0,1,100,1)"),
        0.,
    );
    near(
        call(&b, "DURATION(DATE(2024,1,15),DATE(9999,1,15),0,1,1)"),
        7975.,
    );
}

#[test]
fn nasd_quasi_coupon_work_is_metered_outside_iferror() {
    let b = book();
    near(
        call(
            &b,
            "ACCRINT(DATE(2022,8,28),DATE(2023,8,28),DATE(2023,8,28),0.1,1000,2)",
        ),
        50. * (1. + 178. / 180.),
    );
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(ACCRINT(DATE(1900,1,15),DATE(9999,1,15),DATE(9998,1,15),0.1,1000,4),99)",
            Default::default(),
            EvaluationBudgets {
                run_work: 100,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
