//! Independent Microsoft contracts and mathematical identities; no corpus-derived cases.
//! https://support.microsoft.com/en-us/excel/functions/acot-function and corresponding
//! utility function pages; currency rates are independently recorded in the pure kernel.
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
fn value(out: EvaluationOutcome<CellValue>) -> CellValue {
    match out {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn call(b: &Workbook, text: &str) -> CellValue {
    value(run_formula(
        b,
        0,
        at("Z100").address,
        text,
        Default::default(),
        Default::default(),
    ))
}
fn near(b: &Workbook, text: &str, expected: f64) {
    let CellValue::Number(actual) = call(b, text) else {
        panic!("{text}: {:?}", call(b, text))
    };
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.) * 2e-13,
        "{text}: {actual} != {expected}"
    );
}
#[test]
fn inverse_and_reciprocal_domains_are_mathematical_and_bounded() {
    let b = book();
    for (formula, n) in [
        ("ACOT(0)", std::f64::consts::FRAC_PI_2),
        ("ACOT(1)", std::f64::consts::FRAC_PI_4),
        ("ACOT(-1)", 3. * std::f64::consts::FRAC_PI_4),
        ("ACOTH(2)", 3_f64.ln() / 2.),
        ("ACOTH(-2)", -3_f64.ln() / 2.),
        ("COT(PI()/4)", 1.),
        ("CSC(PI()/2)", 1.),
        ("SEC(0)", 1.),
        ("COTH(1)", 1. / 1_f64.tanh()),
        ("CSCH(-1)", -1. / 1_f64.sinh()),
        ("SECH(0)", 1.),
        ("COTH(1000)", 1.),
        ("SECH(1000)", 0.),
        ("CSCH(-1000)", 0.),
    ] {
        near(&b, formula, n);
    }
    for name in ["COT", "COTH", "CSC", "CSCH"] {
        assert_eq!(
            call(&b, &format!("{name}(0)")),
            CellValue::Error(CellError::Div0)
        );
    }
    for name in ["COT", "COTH", "CSC", "CSCH", "SEC", "SECH"] {
        for n in ["134217728", "-134217728"] {
            assert_eq!(
                call(&b, &format!("{name}({n})")),
                CellValue::Error(CellError::Num)
            );
        }
    }
    for n in ["-1", "0", "1"] {
        assert_eq!(
            call(&b, &format!("ACOTH({n})")),
            CellValue::Error(CellError::Num)
        );
    }
    assert_eq!(
        call(&b, "ACOT(\"bad\")"),
        CellValue::Error(CellError::Value)
    );
    near(&b, "ACOT(TRUE)", std::f64::consts::FRAC_PI_4);
}
#[test]
fn numeric_lifting_broadcasts_and_retains_member_errors() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, 1.).unwrap();
    near(&b, "INDEX(ISO.CEILING({1.1;3.1},{1,2}),2,2)", 4.);
    near(
        &b,
        "INDEX(ACOT({1,-1}),1,2)",
        3. * std::f64::consts::FRAC_PI_4,
    );
    near(&b, "ROWS(ACOT({1;2;3}))", 3.);
    near(&b, "COLUMNS(ISO.CEILING({1.1;2.1},{1,2,3}))", 3.);
    assert_eq!(
        call(&b, "INDEX(CSC({0;1}),1,1)"),
        CellValue::Error(CellError::Div0)
    );
    assert_eq!(
        call(&b, "INDEX(ISO.CEILING({1;2},{1;2;3}),3,1)"),
        CellValue::Error(CellError::NA)
    );
    near(&b, "ACOT(A1)", std::f64::consts::FRAC_PI_4);
    near(&b, "ACOT({1})", std::f64::consts::FRAC_PI_4);
    for (formula, n) in [
        ("ISO.CEILING(-4.3,-2)", -4.),
        ("ISO.CEILING(4.3)", 5.),
        ("ISO.CEILING(4.3,)", 5.),
        ("ISO.CEILING(4.3,B1)", 0.),
        ("ISO.CEILING(0.3,0.1)", 0.3),
    ] {
        near(&b, formula, n);
    }
    assert_eq!(call(&b, "ACOT(#REF!)"), CellValue::Error(CellError::Ref));
    assert_eq!(
        call(&b, "ACOT(RAND(),1)"),
        CellValue::Error(CellError::Value)
    );
    let out = run_formula(
        &b,
        0,
        at("Z100").address,
        "ACOT(RAND(),1)",
        Default::default(),
        Default::default(),
    );
    assert!(out.usage().random_accesses.is_empty());
    near(&b, "IF(FALSE,ACOT(RAND()),7)", 7.);
}

#[test]
fn typed_math_execution_preserves_ordered_errors_and_resource_aborts() {
    let mut b = book();
    b.sheet_mut(0)
        .unwrap()
        .set_value(at("A1").address, 2.)
        .unwrap();
    for formula in [
        "ISO.CEILING(3.1)",
        "ISO.CEILING(3.1,)",
        "ISO.CEILING(3.1,2)",
        "ISO.CEILING(3.1,A1)",
    ] {
        assert_eq!(call(&b, formula), CellValue::Number(4.), "{formula}");
    }
    assert_eq!(call(&b, "ISO.CEILING(3.1,B1)"), CellValue::Number(0.));
    assert_eq!(
        call(&b, "INDEX(ISO.CEILING({1.1;2.1},{2,#VALUE!}),1,1)"),
        CellValue::Number(2.)
    );
    assert_eq!(
        call(&b, "INDEX(ISO.CEILING({1.1;2.1},{2,#VALUE!}),2,2)"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        call(&b, "ISO.CEILING(1.1,\"bad\")"),
        CellValue::Error(CellError::Value)
    );
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(SUM(ACOT(SEQUENCE(100))),99)",
            Default::default(),
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn multinomial_reduces_collections_without_factorial_overflow() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, v) in [("A1", 2.), ("A2", 3.), ("A3", 4.)] {
        s.set_value(at(cell).address, v).unwrap();
    }
    for (formula, n) in [
        ("MULTINOMIAL(2,3,4)", 1260.),
        ("MULTINOMIAL(A1:A3)", 1260.),
        ("MULTINOMIAL({2,3;4,0})", 1260.),
        ("MULTINOMIAL(180,1)", 181.),
        ("MULTINOMIAL(2.9,3.9)", 10.),
        ("MULTINOMIAL(1e300)", 1.),
    ] {
        near(&b, formula, n);
    }
    for (formula, error) in [
        ("MULTINOMIAL(-0.1)", CellError::Num),
        ("MULTINOMIAL(\"bad\")", CellError::Value),
        ("MULTINOMIAL(1000,1000)", CellError::Num),
        ("MULTINOMIAL(#N/A)", CellError::NA),
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(error));
    }
}
#[test]
fn xor_reduces_parity_and_ignores_indirect_text_and_blanks() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("A1").address, "text").unwrap();
    s.set_value(at("A2").address, true).unwrap();
    s.set_value(at("A3").address, 3.).unwrap();
    for (formula, expected) in [
        ("XOR(TRUE,FALSE,TRUE)", false),
        ("XOR(TRUE,TRUE,TRUE)", true),
        ("XOR(A1:A4)", false),
        ("XOR(A1)", false),
    ] {
        if formula == "XOR(A1)" {
            assert_eq!(call(&b, formula), CellValue::Error(CellError::Value));
        } else {
            assert_eq!(call(&b, formula), CellValue::Boolean(expected));
        }
    }
    assert_eq!(
        call(&b, "XOR({TRUE,\"text\",FALSE})"),
        CellValue::Boolean(true)
    );
    assert_eq!(
        call(&b, "XOR(\"text\")"),
        CellValue::Error(CellError::Value)
    );
    assert_eq!(
        call(&b, "XOR(TRUE,#REF!)"),
        CellValue::Error(CellError::Ref)
    );
    assert_eq!(call(&b, "XOR()"), CellValue::Error(CellError::Value));
}
#[test]
fn iso_weeks_cross_calendar_years_and_both_date_systems() {
    let mut b = book();
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        b.date_system = system;
        for (date, n) in [
            ("DATE(2021,1,1)", 53.),
            ("DATE(2021,1,4)", 1.),
            ("DATE(2018,12,31)", 1.),
            ("DATE(2020,12,31)", 53.),
        ] {
            near(&b, &format!("ISOWEEKNUM({date})"), n);
        }
        // Serial 44197 is 2021-01-01 in 1900 and 2025-01-02 in 1904.
        near(
            &b,
            "INDEX(ISOWEEKNUM({44197;44200}),1,1)",
            if system == DateSystem::Excel1900 {
                53.
            } else {
                1.
            },
        );
        assert_eq!(call(&b, "ISOWEEKNUM(-1)"), CellValue::Error(CellError::Num));
    }
}
#[test]
fn networkdays_masks_sign_holidays_and_fractional_dates() {
    let b = book();
    for (formula, n) in [
        ("NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,7))", 5.),
        ("NETWORKDAYS.INTL(DATE(2024,1,7),DATE(2024,1,1))", -5.),
        (
            "NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,7),\"1111111\")",
            0.,
        ),
        (
            "NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,7),\"0000000\")",
            7.,
        ),
        ("NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,7),11)", 6.),
        (
            "NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,7),,{45292;45292;45293})",
            3.,
        ),
        ("NETWORKDAYS.INTL(45292.9,45292.1)", 1.),
        ("INDEX(NETWORKDAYS.INTL({45292;45293},45298),2,1)", 4.),
    ] {
        near(&b, formula, n);
    }
    for (weekend, error) in [
        ("0", CellError::Num),
        ("8", CellError::Num),
        ("\"000000\"", CellError::Value),
        ("\"000000x\"", CellError::Value),
    ] {
        assert_eq!(
            call(&b, &format!("NETWORKDAYS.INTL(45292,45298,{weekend})")),
            CellValue::Error(error)
        );
    }
    assert_eq!(
        call(&b, "NETWORKDAYS.INTL(45292,45298,A1)"),
        CellValue::Error(CellError::Num)
    );
    assert_eq!(
        call(&b, "NETWORKDAYS.INTL(-1,2)"),
        CellValue::Error(CellError::Num)
    );
    near(&b, "NETWORKDAYS.INTL(0,0,\"0000000\")", 1.);
    near(&b, "NETWORKDAYS.INTL(59,61,\"0000000\")", 3.);
}
#[test]
fn fixed_euro_rates_rounding_and_triangulation() {
    let b = book();
    for (formula, n) in [
        ("EUROCONVERT(1.2,\"DEM\",\"EUR\")", 0.61),
        ("EUROCONVERT(1,\"FRF\",\"EUR\",TRUE,3)", 0.152),
        ("EUROCONVERT(1,\"FRF\",\"DEM\",TRUE,3)", 0.29728616),
        ("EUROCONVERT(1.234,\"DEM\",\"DEM\")", 1.234),
        ("EUROCONVERT(1,\"EUR\",\"BEF\")", 40.),
        ("EUROCONVERT(1,\"EUR\",\"PTE\")", 200.48),
        ("EUROCONVERT(-1.2,\"DEM\",\"EUR\")", -0.61),
        ("EUROCONVERT(1,\"eur\",\"sit\",TRUE)", 239.64),
    ] {
        near(&b, formula, n);
    }
    for formula in [
        "EUROCONVERT(1,\"USD\",\"EUR\")",
        "EUROCONVERT(1,\"DEM\",\"EUR\",TRUE,2)",
        "EUROCONVERT(1,\"DEM\",\"EUR\",TRUE,3.5)",
    ] {
        assert_eq!(call(&b, formula), CellValue::Error(CellError::Value));
    }
    near(&b, "EUROCONVERT(1.2,\"DEM\",\"EUR\",,)", 0.61);
}
#[test]
fn identity_arrays_spill_child_first_shrink_and_invalidate_in_both_evaluators() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    s.set_value(at("J1").address, 3.).unwrap();
    s.set_formula(at("A1").address, "MUNIT(J1)").unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    assert_eq!(direct.cell(at("C3")).unwrap(), 1.0.into());
    assert_eq!(direct.cell(at("B3")).unwrap(), 0.0.into());
    direct.set_value(at("J1"), 2.).unwrap();
    assert_eq!(direct.cell(at("C3")).unwrap(), CellValue::Blank);
    assert_eq!(direct.cell(at("B2")).unwrap(), 1.0.into());
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C3"))
            .unwrap(),
        1.0.into()
    );
    overlay.set_value(at("J1"), 2.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("C3"))
            .unwrap(),
        CellValue::Blank
    );
    overlay.set_value(at("B1"), 9.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("A1"))
            .unwrap(),
        CellValue::Error(CellError::Spill)
    );
    overlay.set_value(at("B1"), CellValue::Blank).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        1.0.into()
    );
    let bytes = postcard::to_allocvec(&overlay.snapshot()).unwrap();
    let (book, state) = Overlay::from_snapshot(postcard::from_bytes(&bytes).unwrap()).unwrap();
    let mut restored = state.attach(&book).unwrap();
    assert_eq!(
        restored
            .evaluation(&context)
            .unwrap()
            .cell(at("B2"))
            .unwrap(),
        1.0.into()
    );
}
#[test]
fn utility_shapes_profiles_stored_aliases_and_budgets() {
    let b = book();
    for call_text in [
        "ACOT(1)",
        "ACOTH(2)",
        "COT(1)",
        "COTH(1)",
        "CSC(1)",
        "CSCH(1)",
        "SEC(1)",
        "SECH(1)",
        "ISO.CEILING(1)",
        "ISOWEEKNUM(45292)",
        "MULTINOMIAL(2,3)",
        "MUNIT(2)",
        "NETWORKDAYS.INTL(45292,45298)",
        "EUROCONVERT(1,\"DEM\",\"EUR\")",
        "XOR(TRUE,FALSE)",
    ] {
        let pinned = run_formula_with_configuration(
            &b,
            0,
            at("Z100").address,
            call_text,
            &RunConfiguration::default().with_builtin_policy(BuiltinSemanticsPolicy::PinnedPoi),
            Default::default(),
        );
        assert_eq!(
            value(pinned),
            CellValue::Error(CellError::Name),
            "{call_text}"
        );
        assert_eq!(
            call(&b, &format!("_xlfn.{call_text}")),
            call(&b, call_text),
            "{call_text}"
        );
    }
    for formula in ["MUNIT(0)", "MUNIT(-1)", "MUNIT(16385)"] {
        assert_eq!(call(&b, formula), CellValue::Error(CellError::Value));
    }
    near(&b, "SUM(MUNIT({2}))", 2.);
    near(&b, "SUM(MUNIT(2.9))", 2.);
    let budgets = EvaluationBudgets {
        materialized_values: 20,
        ..Default::default()
    };
    let out = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(INDEX(MUNIT(100),1,1),99)",
        Default::default(),
        budgets,
    );
    assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
    let out = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(SUM(ACOT(SEQUENCE(100))),99)",
        Default::default(),
        budgets,
    );
    assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
}

#[test]
fn lifted_utilities_observe_references_and_holiday_edits() {
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, n) in [
        ("A1", 1.),
        ("A2", 0.),
        ("J1", 45292.),
        ("J2", 45298.),
        ("L1", 45293.),
    ] {
        s.set_value(at(cell).address, n).unwrap();
    }
    s.set_formula(at("D1").address, "ACOT(A1:A2)").unwrap();
    s.set_formula(at("G1").address, "NETWORKDAYS.INTL(J1,J2,1,L1:L4)")
        .unwrap();
    let context = EvaluationContext::default();
    let mut direct = Evaluation::with_workbook(&b)
        .build(context.clone())
        .unwrap();
    assert_eq!(
        direct.cell(at("D2")).unwrap(),
        std::f64::consts::FRAC_PI_2.into()
    );
    assert_eq!(direct.cell(at("G1")).unwrap(), 4.0.into());
    direct.set_value(at("L2"), 45294.).unwrap();
    assert_eq!(direct.cell(at("G1")).unwrap(), 3.0.into());
    let mut overlay = Overlay::with_workbook(b).build().unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        std::f64::consts::FRAC_PI_2.into()
    );
    overlay.set_value(at("A2"), 1.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("D2"))
            .unwrap(),
        std::f64::consts::FRAC_PI_4.into()
    );
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        4.0.into()
    );
    overlay.set_value(at("L2"), 45294.).unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(at("G1"))
            .unwrap(),
        3.0.into()
    );
}
#[test]
fn documented_argument_limits_and_error_consumption() {
    let b = book();
    for (name, limit) in [("XOR", 254), ("MULTINOMIAL", 255)] {
        let valid = format!("{name}({})", vec!["1"; limit].join(","));
        assert!(!matches!(
            call(&b, &valid),
            CellValue::Error(CellError::Value)
        ));
        let invalid = format!("{name}({})", vec!["RAND()"; limit + 1].join(","));
        let out = run_formula(
            &b,
            0,
            at("Z100").address,
            &invalid,
            Default::default(),
            Default::default(),
        );
        assert!(out.usage().random_accesses.is_empty());
        assert_eq!(value(out), CellValue::Error(CellError::Value));
    }
    let budgets = EvaluationBudgets {
        cell_work: 10,
        ..Default::default()
    };
    let out = run_formula(
        &b,
        0,
        at("Z100").address,
        "IFERROR(XOR(TRUE,SUM(SEQUENCE(100))),99)",
        Default::default(),
        budgets,
    );
    assert!(matches!(out, EvaluationOutcome::Aborted { .. }));
}

#[test]
fn documented_abs_spills_and_explicit_intersection_retains_scalar_policy() {
    // Independently authored controls from Microsoft's dynamic-array and @ contracts:
    // https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
    // https://support.microsoft.com/en-us/excel/implicit-intersection-operator
    // https://support.microsoft.com/en-us/office/abs-function-3420200f-5628-4e8c-99da-c99d7c87713c
    // Retrieved 2026-10-25; no upstream code, data or tests reused.
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, number) in [("A1", -2.), ("A2", -3.), ("A3", -5.)] {
        s.set_value(at(cell).address, number).unwrap();
    }
    s.set_formula(at("C2").address, "ABS(A1:A3)").unwrap();
    s.set_formula(at("E2").address, "ABS(@A1:A3)").unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    let mut incremental = overlay.evaluation(&context).unwrap();
    // Child-first access exercises inferred spill-candidate metadata.
    for (cell, expected) in [("C4", 5.), ("C3", 3.), ("C2", 2.), ("E2", 3.)] {
        assert_eq!(
            direct.cell(at(cell)).unwrap(),
            CellValue::Number(expected),
            "direct {cell}"
        );
        assert_eq!(
            incremental.cell(at(cell)).unwrap(),
            CellValue::Number(expected),
            "overlay {cell}"
        );
    }
    assert_eq!(direct.cell(at("E3")).unwrap(), CellValue::Blank);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    let expected = 2.;
    assert_eq!(
        value(run_formula_with_configuration(
            &b,
            0,
            at("G2").address,
            "ABS(A1:A3)",
            &configuration,
            Default::default()
        )),
        CellValue::Number(expected)
    );
    for (formula, expected) in [
        ("SUM(ABS({-2;3;-5}))", CellValue::Number(10.)),
        ("ABS({-7})", CellValue::Number(7.)),
        ("ABS()", CellValue::Error(CellError::Value)),
        (
            "ABS(1,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
        ("ABS(\"bad\")", CellValue::Error(CellError::Value)),
        (
            "INDEX(ABS({-2;#N/A;-5}),2,1)",
            CellValue::Error(CellError::NA),
        ),
    ] {
        assert_eq!(call(&b, formula), expected, "{formula}");
    }
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(SUM(ABS(SEQUENCE(100))),99)",
            Default::default(),
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}

#[test]
fn typed_business_days_broadcast_masks_but_reduce_optional_holidays() {
    // Independently authored Monday 2024-01-01 through Sunday 2024-01-07 controls:
    // Microsoft documents inclusive endpoints, weekend codes and excluded holidays.
    // https://support.microsoft.com/en-us/excel/functions/networkdays-intl-function
    // Mismatch padding and collection-first error precedence are existing Poirot controls.
    let b = book();
    for (formula, expected) in [
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293},45298,{1,11},{45293;45295;45293}),1,1)",
            CellValue::Number(3.),
        ),
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293},45298,{1,11},{45293;45295;45293}),1,2)",
            CellValue::Number(4.),
        ),
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293},45298,{1,11},{45293;45295;45293}),2,1)",
            CellValue::Number(2.),
        ),
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293},45298,{1,11},{45293;45295;45293}),2,2)",
            CellValue::Number(3.),
        ),
        ("NETWORKDAYS.INTL(45292,45298,)", CellValue::Number(5.)),
        ("NETWORKDAYS.INTL(45292,45298,,)", CellValue::Number(5.)),
        (
            "ROWS(NETWORKDAYS.INTL(45292,45298,1,{45293;45295;45293}))",
            CellValue::Number(1.),
        ),
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293;45294},{45298;45299}),3,1)",
            CellValue::Error(CellError::NA),
        ),
        // Isolated date errors independently corroborated in LibreOffice 26.2.6.3.
        (
            "NETWORKDAYS.INTL(1/0,45298)",
            CellValue::Error(CellError::Div0),
        ),
        (
            "NETWORKDAYS.INTL(45292,NA())",
            CellValue::Error(CellError::NA),
        ),
        (
            "INDEX(NETWORKDAYS.INTL({45292;45293},45298,{1,\"bad\"}),2,2)",
            CellValue::Error(CellError::Value),
        ),
    ] {
        assert_eq!(call(&b, formula), expected, "{formula}");
    }
}

#[test]
fn documented_sign_spills_and_explicit_intersection_retains_scalar_policy() {
    // Independently authored controls from Microsoft's dynamic-array and @ contracts:
    // https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
    // https://support.microsoft.com/en-us/excel/implicit-intersection-operator
    // https://support.microsoft.com/en-gb/excel/functions/sign-function
    // Retrieved 2026-10-25; no upstream code, data or tests reused.
    let mut b = book();
    let s = b.sheet_mut(0).unwrap();
    for (cell, number) in [("A1", -2.), ("A2", 0.), ("A3", 5.)] {
        s.set_value(at(cell).address, number).unwrap();
    }
    s.set_formula(at("C2").address, "SIGN(A1:A3)").unwrap();
    s.set_formula(at("E2").address, "SIGN(@A1:A3)").unwrap();
    let mut direct = Evaluation::with_workbook(&b)
        .build(Default::default())
        .unwrap();
    let mut overlay = Overlay::with_workbook(&b).build().unwrap();
    let context = EvaluationContext::default();
    let mut incremental = overlay.evaluation(&context).unwrap();
    // Child-first access exercises inferred spill-candidate metadata.
    for (cell, expected) in [("C4", 1.), ("C3", 0.), ("C2", -1.), ("E2", 0.)] {
        assert_eq!(
            direct.cell(at(cell)).unwrap(),
            CellValue::Number(expected),
            "direct {cell}"
        );
        assert_eq!(
            incremental.cell(at(cell)).unwrap(),
            CellValue::Number(expected),
            "overlay {cell}"
        );
    }
    assert_eq!(direct.cell(at("E3")).unwrap(), CellValue::Blank);
    let configuration =
        RunConfiguration::default().with_array_result_policy(ArrayResultPolicy::ScalarIntersection);
    let expected = -1.;
    assert_eq!(
        value(run_formula_with_configuration(
            &b,
            0,
            at("G2").address,
            "SIGN(A1:A3)",
            &configuration,
            Default::default()
        )),
        CellValue::Number(expected)
    );
    for (formula, expected) in [
        ("SUM(SIGN({-2;3;-5}))", CellValue::Number(-1.)),
        ("SIGN({-7})", CellValue::Number(-1.)),
        ("SIGN()", CellValue::Error(CellError::Value)),
        (
            "SIGN(1,UNKNOWN_FUNCTION())",
            CellValue::Error(CellError::Value),
        ),
        ("SIGN(\"bad\")", CellValue::Error(CellError::Value)),
        (
            "INDEX(SIGN({-2;#N/A;-5}),2,1)",
            CellValue::Error(CellError::NA),
        ),
    ] {
        assert_eq!(call(&b, formula), expected, "{formula}");
    }
    assert!(matches!(
        run_formula(
            &b,
            0,
            at("Z100").address,
            "IFERROR(SUM(SIGN(SEQUENCE(100))),99)",
            Default::default(),
            EvaluationBudgets {
                materialized_values: 20,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
}
