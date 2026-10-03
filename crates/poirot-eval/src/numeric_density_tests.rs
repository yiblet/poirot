use super::*;
fn book() -> Workbook {
    let mut w = Workbook::new();
    w.add_sheet("S").unwrap();
    w
}
fn at(col: u32) -> CellAddress {
    CellAddress { row: 0, col }
}
fn c() -> RunConfiguration {
    RunConfiguration::default().with_numeric_policy(NumericOutcomePolicy::PinnedPoi7321209)
}
fn run(w: &Workbook, f: &str) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(w, 0, at(25), f, &c(), EvaluationBudgets::default())
}
fn value(o: EvaluationOutcome<CellValue>) -> CellValue {
    match o {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{other:?}"),
    }
}
fn bits(v: CellValue) -> u64 {
    match v {
        Number(n) => n.to_bits(),
        other => panic!("{other:?}"),
    }
}
#[test]
fn tiny_string_density_is_raw_number_through_actual_aliases_and_consumers() {
    let mut w = book();
    for name in ["NORMDIST", "NORM.DIST"] {
        let f = format!("{name}(0,0,\"5e-324\",FALSE)");
        assert_eq!(bits(value(run(&w, &f))), f64::INFINITY.to_bits());
        let plan = bound::FormulaPlan::bind(&poirot_formula::parse_formula(&f).unwrap()).unwrap();
        assert_eq!(
            bits(value(run_plan_with_configuration(
                &w,
                0,
                at(25),
                &plan,
                &c(),
                EvaluationBudgets::default()
            ))),
            f64::INFINITY.to_bits()
        );
        assert_eq!(
            bits(value(run(&w, &format!("IFERROR({f},42)")))),
            f64::INFINITY.to_bits()
        );
        assert_eq!(value(run(&w, &format!("ISNUMBER({f})"))), Boolean(true));
        for wrapped in [format!("ABS({f})"), format!("SUM({f})"), format!("{f}+0")] {
            assert_eq!(value(run(&w, &wrapped)), Error(Num));
        }
        assert_eq!(
            value(run_formula_with_configuration(
                &w,
                0,
                at(25),
                &f,
                &RunConfiguration::default(),
                EvaluationBudgets::default()
            )),
            Error(Num)
        );
        assert_eq!(
            bits(value(run(&w, &format!("{name}(0,0,\"5e-324\",TRUE)")))),
            0.5f64.to_bits()
        );
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::from_bits(1)))
        .unwrap();
    assert_eq!(value(run(&w, "NORM.DIST(0,0,A1,FALSE)")), Error(Num));
    w.sheet_mut(0)
        .unwrap()
        .set_formula(at(0), "NORM.DIST(0,0,\"5e-324\",FALSE)", Blank)
        .unwrap();
    assert_eq!(bits(value(run(&w, "A1"))), f64::INFINITY.to_bits());
}
#[test]
fn actual_special_string_projection_ignores_infinity_sign_and_canonicalizes_nan_only_there() {
    let mut w = book();
    let projected = "1.7976931348623E+308".parse::<f64>().unwrap().to_bits();
    for infinity in [f64::INFINITY, f64::NEG_INFINITY] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(at(0), Number(infinity))
            .unwrap();
        for f in ["NORMINV(0.5,A1,1)", "NORM.INV(0.5,A1,1)"] {
            assert_eq!(bits(value(run(&w, f))), projected);
        }
        for f in [
            "NORMSDIST(A1)",
            "NORM.S.DIST(A1)",
            "NORMDIST(A1,0,1,TRUE)",
            "NORM.DIST(A1,0,1,TRUE)",
        ] {
            assert_eq!(bits(value(run(&w, f))), 1.0f64.to_bits());
        }
        assert_eq!(bits(value(run(&w, "NORM.DIST(0,0,A1,FALSE)"))), 0);
        assert_eq!(value(run(&w, "NORM.DIST(A1,#DIV/0!,1,FALSE)")), Error(Div0));
    }
    for raw in [
        0x7ff8_0000_0000_0042,
        0xfff8_0000_0000_0099,
        0x7ff0_0000_0000_0001,
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(at(0), Number(f64::from_bits(raw)))
            .unwrap();
        assert_eq!(
            bits(value(run(&w, "A1"))),
            raw,
            "numeric reference does not canonicalize payload"
        );
        for f in [
            "NORM.DIST(A1,#DIV/0!,1,FALSE)",
            "NORMDIST(A1,#DIV/0!,1,FALSE)",
            "NORMSDIST(A1)",
            "NORM.S.DIST(A1)",
            "NORMSINV(A1)",
        ] {
            assert_eq!(value(run(&w, f)), Error(VALUE_ERROR));
        }
    }
}
#[test]
fn actual_cumulative_nan_is_number_and_source_argument_order_stays_lazy() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::MAX))
        .unwrap();
    for name in ["NORM.DIST", "NORMDIST"] {
        let f = format!("{name}(A1,-A1,A1,TRUE)");
        assert!(matches!(value(run(&w,&f)),Number(n) if n.is_nan()));
        assert!(matches!(value(run(&w,&format!("IFERROR({f},42)"))),Number(n) if n.is_nan()));
        assert_eq!(value(run(&w, &format!("ISNUMBER({f})"))), Boolean(true));
        assert_eq!(value(run(&w, &format!("ABS({f})"))), Error(Num));
        assert_eq!(value(run(&w, &format!("IF(FALSE,{f},1)"))), Number(1.0));
    }
    for f in [
        "NORM.DIST(0,0,0,POISSON(1,0,FALSE))",
        "NORMDIST(0,0,0,POISSON(1,0,FALSE))",
    ] {
        assert_eq!(value(run(&w, f)), Error(Num));
    }
    assert_eq!(value(run(&w, "NORM.DIST(0,0,1,#DIV/0!)")), Error(Div0));
    assert_eq!(
        value(run(&w, "IFERROR(1,NORM.DIST(0,0,1,POISSON(1,0,FALSE)))")),
        Number(1.0)
    );
    assert_eq!(
        bits(value(run(&w, "NORM.DIST(0,0,1,TRUE)"))),
        0.5f64.to_bits()
    );
    assert!(
        matches!(value(run(&w,"NORM.DIST(0,0,1,FALSE)")),Number(n) if n.is_finite()&&n>0.0&&n<1.0)
    );
}
#[test]
fn actual_selected_density_projection_obeys_text_budget_and_gamma_meter() {
    let mut w = book();
    w.sheet_mut(0)
        .unwrap()
        .set_value(at(0), Number(f64::INFINITY))
        .unwrap();
    let budgets = EvaluationBudgets {
        text_bytes: 31,
        ..EvaluationBudgets::default()
    };
    assert!(matches!(
        run_formula_with_configuration(&w, 0, at(25), "NORM.S.DIST(A1)", &c(), budgets),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(BudgetFailure {
                kind: BudgetKind::TextBytes,
                ..
            }),
            ..
        }
    ));
    let ordinary = run(&book(), "NORM.DIST(1,0,1,TRUE)");
    let density = run(&book(), "NORM.DIST(1,0,1,FALSE)");
    assert!(
        ordinary.usage().helper_operations > density.usage().helper_operations,
        "same numeric/coercion inputs and fixedwork512; cumulative charges actual Gamma iterations"
    );
    assert!(matches!(ordinary, EvaluationOutcome::Complete { .. }));
    assert!(matches!(density, EvaluationOutcome::Complete { .. }));
}

#[test]
fn actual_selected_nan_boolean_coercion_precedes_normal_and_poisson_results() {
    let w = book();
    let nan = "NORM.DIST(\"1.7976931348623157e308\",\"-1.7976931348623157e308\",\"1.7976931348623157e308\",TRUE)";
    for name in ["NORMDIST", "NORM.DIST"] {
        let f = format!("{name}(0,0,1,{nan})");
        assert_eq!(value(run(&w, &f)), Error(VALUE_ERROR));
        assert_eq!(value(run(&w, &format!("IFERROR({f},42)"))), Number(42.0));
    }
    for name in ["POISSON", "POISSON.DIST"] {
        for mean in [0, 1] {
            let f = format!("{name}(1,{mean},{nan})");
            assert_eq!(value(run(&w, &f)), Error(VALUE_ERROR));
            assert_eq!(value(run(&w, &format!("IFERROR({f},42)"))), Number(42.0));
        }
    }
}
#[test]
fn actual_operand_boolean_consumers_reject_nan_but_lookup_range_flag_accepts_it() {
    let mut w = book();
    let nan = "NORM.DIST(\"1.7976931348623157e308\",\"-1.7976931348623157e308\",\"1.7976931348623157e308\",TRUE)";
    for f in [
        format!("IF({nan},POISSON(1,0,FALSE),POISSON(1,0,FALSE))"),
        format!("NOT({nan})"),
        format!("AND(FALSE,{nan})"),
        format!("OR(TRUE,{nan})"),
    ] {
        assert_eq!(value(run(&w, &f)), Error(VALUE_ERROR));
        assert_eq!(value(run(&w, &format!("IFERROR({f},42)"))), Number(42.));
    }
    for f in [
        "IF(NORMSINV(1E-300),42,POISSON(1,0,FALSE))",
        "IF(NORM.DIST(0,0,\"5e-324\",FALSE),42,POISSON(1,0,FALSE))",
    ] {
        assert_eq!(value(run(&w, f)), Number(42.));
    }
    for (row, col, n) in [(0, 0, 1.), (1, 0, 2.), (0, 1, 10.), (1, 1, 20.)] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col }, Number(n))
            .unwrap();
    }
    // Exact lookup misses 1.5; approximate range-flag NaN must choose the preceding key.
    assert_eq!(
        value(run(&w, &format!("VLOOKUP(1.5,A1:B2,2,{nan})"))),
        Number(10.)
    );
    assert_eq!(
        value(run(&w, &format!("HLOOKUP(1.5,A1:A2,2,{nan})"))),
        Number(2.)
    );
}
#[test]
fn actual_gamma_admission_is_operational_under_iferror_and_nan_guard_has_no_iteration() {
    let w = book();
    let control = run(&w, "IFERROR(NORM.DIST(1,0,1,0),42)");
    assert!(matches!(control, EvaluationOutcome::Complete { .. }));
    let budgets = EvaluationBudgets {
        run_work: control.usage().work_units,
        ..Default::default()
    };
    assert!(matches!(
        run_formula_with_configuration(
            &w,
            0,
            at(25),
            "IFERROR(NORM.DIST(1,0,1,1),42)",
            &c(),
            budgets
        ),
        EvaluationOutcome::Aborted {
            failure: EvaluationFailure::Budget(_),
            ..
        }
    ));
    let args = "\"1.7976931348623157e308\",\"-1.7976931348623157e308\",\"1.7976931348623157e308\"";
    let cumulative = run(&w, &format!("NORM.DIST({args},1)"));
    let density = run(&w, &format!("NORM.DIST({args},0)"));
    assert!(matches!(cumulative,EvaluationOutcome::Complete {value:Number(n),..} if n.is_nan()));
    assert_eq!(
        cumulative.usage().helper_operations,
        density.usage().helper_operations
    );
}
