use super::*;
fn book(system: DateSystem) -> Workbook {
    let mut w = Workbook::new();
    w.date_system = system;
    w.add_sheet("Sheet1").unwrap();
    w
}
fn evaluate(w: &Workbook, f: &str) -> CellValue {
    match crate::run_formula(
        w,
        0,
        "Z20".parse().unwrap(),
        f,
        EvaluationInputs {
            as_of_time: UNIX_EPOCH + Duration::from_secs(1609502400),
            randomness_seed: 0,
            localization: Localization {
                utc_offset_seconds: 0,
                date_order: DateOrder::MonthDayYear,
            },
        },
        Default::default(),
    ) {
        crate::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    }
}
fn num(w: &Workbook, f: &str, n: f64) {
    assert_eq!(evaluate(w, f), Number(n), "{f}");
}
#[test]
fn calendar_epoch_and_serial_boundaries() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        for n in [0, 1, 31, 59, 60, 61, 1462, 40000, max_serial(system)] {
            let d = date(n as f64, system).unwrap();
            let back = serial(d, system).unwrap();
            assert_eq!(
                back,
                if system == DateSystem::Excel1900 && n == 60 {
                    61.
                } else {
                    n as f64
                }
            );
        }
        let w = book(system);
        num(&w, &format!("YEAR({})", max_serial(system)), 9999.);
        assert_eq!(
            evaluate(&w, &format!("DAY({})", max_serial(system) + 1)),
            Error(Num)
        );
    }
    let w = book(DateSystem::Excel1900);
    for (f, n) in [
        ("DATE(1900,2,29)", 60.),
        ("DATE(4,1,1)", 1462.),
        ("DATE(1900,222,1)", 6727.),
        ("DATE(1900,2,0)", 31.),
        ("DATE(1900,1,60)", 59.),
        ("YEAR(0)", 1900.),
        ("MONTH(0)", 1.),
        ("DAY(0)", 0.),
        ("DAY(60)", 1.),
        ("DAY(61)", 1.),
        ("DAYS(61,60)", 0.),
        ("EOMONTH(0,0)", 31.),
        ("EDATE(0,0)", 0.),
    ] {
        num(&w, f, n);
    }
    let w = book(DateSystem::Excel1904);
    num(&w, "DATE(1904,1,1)", 0.);
    num(&w, "DAY(0)", 1.);
    num(&w, "YEAR(0)", 1904.);
}
#[test]
fn time_and_deterministic_clock() {
    let w = book(DateSystem::Excel1900);
    for (f, n) in [
        ("TIME(24,0,0)", 0.),
        ("TIME(-1,120,0)", 1. / 24.),
        ("TIME(0,-0.1,60)", 0.),
        ("HOUR(0.5)", 12.),
        ("MINUTE(TIME(12,34,56))", 34.),
        ("SECOND(TIME(12,34,56))", 56.),
        ("TIMEVALUE(\"12:00\")", 0.5),
        // Pinned DateUtil M/dd rejects a one-digit day; DateParser then
        // accepts the original date and TIMEVALUE returns its date-only zero.
        // This differs from the Excel-oriented TimeValue.java Javadoc example.
        ("TIMEVALUE(\"1/1/2008 12:00\")", 0.),
        ("TIMEVALUE(\"1/01/2008 12:00\")", 0.5),
        ("TIMEVALUE(\"1:30 PM\")", 13.5 / 24.),
        ("TIMEVALUE(\"\")", 0.),
        ("DATEVALUE(\"8/22/2011\")", 40777.),
        ("DATEVALUE(\"1/1\")", 44197.),
        ("TODAY()", 44197.),
        ("NOW()", 44197.5),
    ] {
        num(&w, f, n);
    }
    assert_eq!(evaluate(&w, "TIME(32768,0,0)"), Error(VALUE_ERROR));
    assert_eq!(evaluate(&w, "TIME(-1,0,0)"), Error(VALUE_ERROR));
    let options = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    assert_eq!(
        match crate::run_formula(
            &w,
            0,
            "A1".parse().unwrap(),
            "NOW()",
            options,
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Error(Num)
    );
    let options = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1609459200),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: -3600,
            date_order: DateOrder::MonthDayYear,
        },
    };
    assert_eq!(
        match crate::run_formula(
            &w,
            0,
            "A1".parse().unwrap(),
            "TODAY()",
            options,
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Number(44196.)
    );
}
#[test]
fn localization_context_and_recalculation() {
    let mut w = book(DateSystem::Excel1900);
    let mut c = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(1609502400),
        randomness_seed: 42,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    let eval = |w: &Workbook, f: &str, c| match crate::run_formula(
        w,
        0,
        "A1".parse().unwrap(),
        f,
        c,
        Default::default(),
    ) {
        crate::EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    assert_eq!(eval(&w, "DATEVALUE(\"1/2/2021\")", c), Number(44198.));
    c.localization.date_order = DateOrder::DayMonthYear;
    assert_eq!(eval(&w, "DATEVALUE(\"1/2/2021\")", c), Number(44228.));
    assert_eq!(eval(&w, "DATEVALUE(\"2021-02-01\")", c), Number(44228.));
    let first = c.fingerprint();
    c.randomness_seed += 1;
    assert_ne!(first, c.fingerprint());
    c.localization.utc_offset_seconds = i32::MIN;
    assert_eq!(eval(&w, "NOW()", c), Error(Num));
    c.localization.utc_offset_seconds = 0;
    c.as_of_time = UNIX_EPOCH - Duration::from_secs(1);
    assert_eq!(eval(&w, "TODAY()", c), Number(25568.));
    c.as_of_time = UNIX_EPOCH + Duration::from_secs(1_609_502_400);
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A1".parse().unwrap(), "NOW()", Number(777.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_formula("A2".parse().unwrap(), "NOW()", Number(777.))
        .unwrap();
    match crate::run_recalculation(&mut w, c, Default::default()) {
        crate::EvaluationOutcome::Complete { .. } => (),
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    assert_eq!(
        w.sheet(0)
            .unwrap()
            .get("A1".parse().unwrap())
            .unwrap()
            .value,
        Number(44197.5)
    );
    assert_eq!(
        w.sheet(0)
            .unwrap()
            .get("A2".parse().unwrap())
            .unwrap()
            .value,
        Number(44197.5)
    );
    c.as_of_time += Duration::from_secs(86400);
    match crate::run_recalculation(&mut w, c, Default::default()) {
        crate::EvaluationOutcome::Complete { .. } => (),
        outcome => panic!("evaluation aborted: {outcome:?}"),
    };
    assert_eq!(
        w.sheet(0)
            .unwrap()
            .get("A1".parse().unwrap())
            .unwrap()
            .value,
        Number(44198.5)
    );
}
#[test]
fn month_week_and_financial_day_count() {
    let w = book(DateSystem::Excel1900);
    for (f, n) in [
        ("EDATE(DATE(2020,1,31),1)", 43890.),
        ("EOMONTH(DATE(2021,2,12),0)", 44255.),
        ("WEEKDAY(2958465)", 6.),
        ("WEEKDAY(DATE(2021,9,27),2)", 1.),
        ("WEEKDAY(DATE(2021,9,27),3)", 0.),
        ("WEEKNUM(DATE(2021,9,27))", 40.),
        ("WEEKNUM(DATE(2021,9,27),21)", 39.),
        ("DAYS360(DATE(1993,2,28),DATE(1993,3,1),FALSE)", 1.),
        ("DAYS360(DATE(1993,2,28),DATE(1993,3,1),TRUE)", 3.),
        ("DAYS360(DATE(1993,2,28),DATE(1993,2,28))", -2.),
        ("YEARFRAC(1,2958465)", 8100.),
        ("YEARFRAC(DATE(2020,1,1),DATE(2021,1,1),1)", 1.),
        ("YEARFRAC(10,370,2)", 1.),
        ("YEARFRAC(10,375,3)", 1.),
        ("YEARFRAC(DATE(2020,1,1),DATE(2021,1,1),4)", 1.),
    ] {
        num(&w, f, n);
    }
    assert_eq!(evaluate(&w, "EDATE(\"1\",0)"), Error(VALUE_ERROR));
    assert_eq!(evaluate(&w, "WEEKNUM(1,18)"), Error(Num));
    assert_eq!(evaluate(&w, "YEARFRAC(1,2,5)"), Error(Num));
}
#[test]
fn business_days_holidays_and_weekends() {
    let mut w = book(DateSystem::Excel1900);
    for (f, n) in [
        ("NETWORKDAYS(0,0)", 0.),
        ("WORKDAY(0,1)", 1.),
        ("WORKDAY.INTL(\"2013/09/30\",-1)", 41544.),
        ("WORKDAY.INTL(\"2013/09/27\",1)", 41547.),
        ("NETWORKDAYS(DATE(2021,9,27),DATE(2021,10,1))", 5.),
        ("WORKDAY.INTL(DATE(2021,9,27),1,12)", 44467.),
    ] {
        num(&w, f, n);
    }
    w.sheet_mut(0)
        .unwrap()
        .set_value("A1".parse().unwrap(), Number(44467.))
        .unwrap();
    w.sheet_mut(0)
        .unwrap()
        .set_value("A2".parse().unwrap(), Number(44467.))
        .unwrap();
    num(&w, "NETWORKDAYS(DATE(2021,9,27),DATE(2021,10,1),A1:A2)", 3.);
    num(&w, "WORKDAY(DATE(2021,9,27),1,A1:A2)", 44468.);
    assert_eq!(evaluate(&w, "NETWORKDAYS(2,1)"), Error(Name));
    assert_eq!(evaluate(&w, "WORKDAY.INTL(1,1,\"1111111\")"), Error(Num));
    assert_eq!(evaluate(&w, "WORKDAY.INTL(1,1,8)"), Error(Num));
}
#[test]
fn observed_poi_leading_zero_weekend_codes() {
    let w = book(DateSystem::Excel1900);
    num(&w, "WORKDAY.INTL(\"2013/09/27\",1,\"0000011\")", 41545.);
    assert_eq!(
        evaluate(&w, "WORKDAY.INTL(\"2013/09/27\",1,\"0000000\")"),
        Error(Num)
    );
}
#[test]
fn observed_poi_permissive_and_phantom_slot_boundaries() {
    let w = book(DateSystem::Excel1900);
    for (f, n) in [
        ("WORKDAY(59,1)", 61.),
        ("WORKDAY(60,1)", 62.),
        ("WORKDAY(61,-1)", 59.),
        ("WORKDAY(59,1,60)", 62.),
        ("WORKDAY(61,-1,60)", 58.),
        ("YEARFRAC(\"-1\",\"1\",2)", 2. / 360.),
        ("YEARFRAC(\"-1\",\"1\",0)", 1. / 360.),
        ("YEARFRAC(\"2958466\",\"2958467\",2)", 1. / 360.),
    ] {
        num(&w, f, n);
    }
    assert_eq!(evaluate(&w, "YEARFRAC(-1,1,2)"), Error(Num));
}
#[test]
fn exact_poi_operand_contracts_and_error_order() {
    let w = book(DateSystem::Excel1900);
    for f in [
        "EDATE(1,TRUE)",
        "EDATE(1,\"1\")",
        "EDATE(#REF!,0)",
        "EDATE(-1,\"1\")",
        "WEEKNUM(#REF!)",
    ] {
        assert_eq!(evaluate(&w, f), Error(VALUE_ERROR), "{f}");
    }
    assert_eq!(evaluate(&w, "YEARFRAC(#REF!,1,#DIV/0!)"), Error(Div0));
    num(&w, "WORKDAY(44466.25,1)", 44467.25);
    num(&w, "WORKDAY.INTL(44466,1,\"1\")", 44467.);
    num(&w, "DAY(44197.99999999)", 2.);
    num(&w, "HOUR(44197.99999999)", 0.);
    assert_eq!(evaluate(&w, "YEARFRAC(1,365,-0.1)"), Error(Num));
}
#[test]
fn nom_text_boundaries_and_unused_clock() {
    let w = book(DateSystem::Excel1900);
    let invalid = EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::from_secs(300_000_000_000),
        randomness_seed: 0,
        localization: Localization {
            utc_offset_seconds: 0,
            date_order: DateOrder::MonthDayYear,
        },
    };
    for (f, n) in [
        ("DATEVALUE(\"2021-1-2 ignored trailing text\")", 44198.),
        ("DATEVALUE(\"2-Jan-2021\")", 44198.),
        ("TIMEVALUE(\"12:00\")", 0.5),
    ] {
        assert_eq!(
            match crate::run_formula(&w, 0, "A1".parse().unwrap(), f, invalid, Default::default()) {
                crate::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            Number(n)
        );
    }
    for f in [
        "DATEVALUE(\"1/2/21\")",
        "DATEVALUE(\"2021-1-002\")",
        "DATEVALUE(\" 2021-1-2\")",
        "DATEVALUE(\"2021-1-2T12:00\")",
    ] {
        assert_eq!(evaluate(&w, f), Error(VALUE_ERROR), "{f}");
    }
    assert_eq!(
        match crate::run_formula(
            &w,
            0,
            "A1".parse().unwrap(),
            "DATEVALUE(\"1/2\")",
            invalid,
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Error(Num)
    );
}
#[test]
fn week_skip_matches_independent_day_traversal() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for start in [0, 1, 58, 59, 60, 61, 100, 44000] {
            for count in [-400, -19, -1, 0, 1, 19, 400] {
                if start + count * 2 < 0 {
                    continue;
                }
                for mode in [1, 2, 7, 11, 12, 17] {
                    let mut day = epoch(date(start as f64, system).unwrap());
                    let mut remaining: i64 = count;
                    while remaining != 0 {
                        let sign = remaining.signum();
                        day += sign;
                        let dow = (day + 4).rem_euclid(7);
                        let weekend = if mode <= 7 {
                            (dow - (mode + 5) % 7).rem_euclid(7) < 2
                        } else {
                            dow == (mode - 11) % 7
                        };
                        if !weekend {
                            remaining -= sign;
                        }
                    }
                    num(
                        &w,
                        &format!("WORKDAY.INTL({start},{count},{mode})"),
                        serial(civil(day), system).unwrap(),
                    );
                }
            }
        }
    }
}
#[test]
fn all_function_arity_errors_and_references() {
    let w = book(DateSystem::Excel1900);
    for name in FUNCTION_DESCRIPTORS
        .iter()
        .filter(|d| matches!(d.handler, FunctionId::Date(_)))
        .map(|d| d.name)
    {
        assert_eq!(
            evaluate(&w, &format!("{name}(1,2,3,4,5)")),
            Error(VALUE_ERROR),
            "{name}"
        );
        if !matches!(name, "NOW" | "TODAY") {
            assert!(
                matches!(evaluate(&w, &format!("{name}(#REF!)")), Error(_)),
                "{name}"
            );
        }
    }
    for f in [
        "DAY(\"not a date\")",
        "DATEVALUE(\"2021-02-29\")",
        "TIMEVALUE(\"25:00\")",
        "DATEVALUE(\"\")",
    ] {
        assert_eq!(evaluate(&w, f), Error(VALUE_ERROR), "{f}");
    }
    for day in (0..=max_serial(DateSystem::Excel1900)).step_by(7919) {
        let d = date(day as f64, DateSystem::Excel1900).unwrap();
        assert!((1..=9999).contains(&d.y));
        assert!((1..=12).contains(&d.m));
    }
}

#[test]
fn shared_policy_formula_and_display_boundaries() {
    for (system, last) in [
        (DateSystem::Excel1900, 2_958_465),
        (DateSystem::Excel1904, 2_957_003),
    ] {
        let w = book(system);
        assert_eq!(max_serial(system), last);
        for fraction in [0.5, 86399. / 86400.] {
            let n = last as f64 + fraction;
            num(&w, &format!("YEAR({n})"), 9999.);
            num(&w, &format!("MONTH({n})"), 12.);
            num(&w, &format!("DAY({n})"), 31.);
            num(
                &w,
                &format!("HOUR({n})"),
                if fraction == 0.5 { 12. } else { 23. },
            );
            assert_eq!(
                evaluate(&w, &format!("TEXT({n},\"yyyy-mm-dd hh:mm:ss\")")),
                Text(
                    if fraction == 0.5 {
                        "9999-12-31 12:00:00"
                    } else {
                        "9999-12-31 23:59:59"
                    }
                    .into()
                )
            );
        }
        for invalid in [-0.5, last as f64 + 1.] {
            assert_eq!(evaluate(&w, &format!("YEAR({invalid})")), Error(Num));
            assert!(
                poirot_format::format_value(
                    &Number(invalid),
                    &poirot_format::parse_format("yyyy-mm-dd").unwrap(),
                    poirot_format::FormatOptions {
                        date_system: system,
                        width: None
                    },
                )
                .is_err()
            );
        }
        // Valid raw final-day fractions can round into an unsupported year.
        assert_eq!(evaluate(&w, &format!("DAY({}.99999999)", last)), Error(Num));
        assert_eq!(
            evaluate(
                &w,
                &format!("YEARFRAC(\"{}\",\"{}\",2)", last + 1, last + 2)
            ),
            Number(1. / 360.)
        );
        assert_eq!(
            evaluate(&w, &format!("YEARFRAC({}, {},2)", last + 1, last + 2)),
            Error(Num)
        );
    }
    for (system, cases) in [
        (
            DateSystem::Excel1900,
            [
                (0, 1900., 1., 0., "1899-12-31"),
                (59, 1900., 2., 28., "1900-02-28"),
                (60, 1900., 3., 1., "1900-02-29"),
                (61, 1900., 3., 1., "1900-03-01"),
            ],
        ),
        (
            DateSystem::Excel1904,
            [
                (0, 1904., 1., 1., "1904-01-01"),
                (59, 1904., 2., 29., "1904-02-29"),
                (60, 1904., 3., 1., "1904-03-01"),
                (61, 1904., 3., 2., "1904-03-02"),
            ],
        ),
    ] {
        let w = book(system);
        for (n, year, month, day, rendered) in cases {
            num(&w, &format!("YEAR({n})"), year);
            num(&w, &format!("MONTH({n})"), month);
            num(&w, &format!("DAY({n})"), day);
            assert_eq!(
                evaluate(&w, &format!("TEXT({n},\"yyyy-mm-dd\")")),
                Text(rendered.into())
            );
        }
        assert_eq!(evaluate(&w, "TEXT(1.5,\"[h]:mm\")"), Text("36:00".into()));
    }
}
