use poirot::eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot::model::{CellAddress, CellError, CellValue, DateSystem, Workbook};

fn run(
    w: &Workbook,
    source: &str,
    excel: bool,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    let config = RunConfiguration::default().with_builtin_policy(if excel {
        BuiltinSemanticsPolicy::ExcelDocumentation
    } else {
        BuiltinSemanticsPolicy::PinnedPoi
    });
    run_formula_with_configuration(
        w,
        0,
        CellAddress { row: 0, col: 25 },
        source,
        &config,
        budgets,
    )
}
fn value(w: &Workbook, source: &str, excel: bool) -> CellValue {
    match run(w, source, excel, EvaluationBudgets::default()) {
        EvaluationOutcome::Complete { value, .. } => value,
        result => panic!("{source}: {result:?}"),
    }
}
fn book(system: DateSystem) -> Workbook {
    let mut w = Workbook::new();
    w.date_system = system;
    w.add_sheet("Sheet1").unwrap();
    for (row, n) in [1900., 3., 0., 45353., 45352., 45351., 10.75, 8.25]
        .into_iter()
        .enumerate()
    {
        w.sheet_mut(0)
            .unwrap()
            .set_value(
                CellAddress {
                    row: row as u32,
                    col: 0,
                },
                n,
            )
            .unwrap();
    }
    w
}
#[test]
fn date_host_preserves_text_epoch_and_operational_failure_boundaries() {
    // Excel's 1904 epoch is 1,462 days after the 1900 epoch; noon is half a day.
    for (system, serial) in [
        (DateSystem::Excel1900, 45352.),
        (DateSystem::Excel1904, 43890.),
    ] {
        let w = book(system);
        assert_eq!(
            value(&w, "DATEVALUE(\"2024-03-01\")", true),
            CellValue::Number(serial)
        );
        assert_eq!(
            value(&w, "TIMEVALUE(\"12:00 PM\")", true),
            CellValue::Number(0.5)
        );
        assert_eq!(
            value(&w, "IFERROR(TIMEVALUE(\"invalid\"),7)", true),
            CellValue::Number(7.)
        );
        assert!(matches!(
            run(
                &w,
                "IFERROR(TIMEVALUE(\"12:00 PM\"),7)",
                true,
                EvaluationBudgets {
                    text_bytes: 2,
                    ..Default::default()
                }
            ),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
#[test]
fn date_serial_overflow_and_errors_follow_excel_calendar() {
    let w = book(DateSystem::Excel1900);
    for (formula, expected) in [
        ("DATE(1900,1,0)", 0.),
        ("DATE(1900,1,60)", 60.),
        ("DATE(1900,2,29)", 60.),
        ("DATE(1900,3,0)", 60.),
        ("DATE(0,3,0)", 60.),
        ("DATE(A1,A2,A3)", 60.),
        ("DATE(1900,3,-1)", 59.),
        ("DATE(1900,2,30)", 61.),
        ("DATE(1900,0,32)", 1.),
        ("DATE(1900,0.9,32)", 1.),
        ("DATE(2024,14,0)", 45688.),
        ("DATE(9999,12,31)", 2958465.),
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    for formula in [
        "DATE(-1,1,1)",
        "DATE(10000,1,1)",
        "DATE(9999,12,32)",
        "DATE(1900,1,-1)",
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
    for (formula, error) in [
        ("DATE(#REF!,1,1)", CellError::Ref),
        ("DATE(2024,1/0,1)", CellError::Div0),
        ("DATE(2024,1,\"bad\")", CellError::Value),
    ] {
        assert_eq!(value(&w, formula, true), CellValue::Error(error));
    }
    let w = book(DateSystem::Excel1904);
    for (formula, n) in [
        ("DATE(1904,1,1)", 0.),
        ("DATE(1904,3,0)", 59.),
        ("DATE(1904,2,30)", 60.),
        ("DATE(1903,12,32)", 0.),
        ("DATE(9999,12,31)", 2957003.),
    ] {
        assert_eq!(value(&w, formula, true), CellValue::Number(n), "{formula}");
    }
    assert_eq!(
        value(&w, "DATE(1904,1,0)", true),
        CellValue::Error(CellError::Num)
    );
}
#[test]
fn days_serial_subtraction_and_workday_signed_truncation() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for (formula, n) in [
            ("DAYS(A7,A8)", 2.5),
            ("DAYS(8.25,10.75)", -2.5),
            ("DAYS(61,60)", 1.),
            ("DAYS(60,59)", 1.),
            ("DAYS(10.5,10.5)", 0.),
        ] {
            assert_eq!(value(&w, formula, true), CellValue::Number(n), "{formula}");
        }
        assert_eq!(
            value(&w, "DAYS(-1,0)", true),
            CellValue::Error(CellError::Num)
        );
        assert_eq!(
            value(&w, "DAYS(1,#REF!)", true),
            CellValue::Error(CellError::Ref)
        );
        assert_eq!(
            value(&w, "WORKDAY(1,#REF!)", true),
            CellValue::Error(CellError::Ref)
        );
        assert_eq!(
            value(&w, "DAYS(DATE(2024,3,1),\"2024/02/28\")", true),
            CellValue::Number(2.)
        );
        let shift = if system == DateSystem::Excel1904 {
            1462.
        } else {
            0.
        };
        for (formula, n) in [
            ("WORKDAY(DATE(2024,3,1),-1.9)", 45351.),
            ("WORKDAY(DATE(2024,3,1),-0.9)", 45352.),
            ("WORKDAY(DATE(2024,3,1),1.9)", 45355.),
            ("WORKDAY(DATE(2024,3,3),-1.9)", 45352.),
            ("WORKDAY(DATE(2024,3,2),-1.9,DATE(2024,3,1))", 45351.),
            ("WORKDAY.INTL(DATE(2024,3,1),-1.9)", 45351.),
        ] {
            assert_eq!(
                value(&w, formula, true),
                CellValue::Number(n - shift),
                "{formula}"
            );
        }
    }
    let w = book(DateSystem::Excel1900);
    assert_eq!(
        value(&w, "WORKDAY(A4,-1.9,A5:A6)", true),
        CellValue::Number(45350.)
    );
    assert!(matches!(
        run(
            &w,
            "WORKDAY(A4,-1.9,A5:A6)",
            true,
            EvaluationBudgets {
                run_work: 10,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}
#[test]
fn iso_week_rolls_into_both_neighboring_years_in_both_epochs() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for (formula, week) in [
            ("WEEKNUM(DATE(2021,1,1),21)", 53.),
            ("WEEKNUM(DATE(2021,1,3),21)", 53.),
            ("WEEKNUM(DATE(2021,1,4),21)", 1.),
            ("WEEKNUM(DATE(2018,12,31),21)", 1.),
            ("WEEKNUM(DATE(2016,1,1),21)", 53.),
            ("WEEKNUM(DATE(2022,1,1),21)", 52.),
            ("WEEKNUM(DATE(2020,12,31),21)", 53.),
            ("WEEKNUM(DATE(2021,1,1),2)", 1.),
        ] {
            assert_eq!(
                value(&w, formula, true),
                CellValue::Number(week),
                "{formula}"
            );
        }
        assert_eq!(
            value(&w, "WEEKNUM(1,18)", true),
            CellValue::Error(CellError::Num)
        );
        assert_eq!(
            value(&w, "WEEKNUM(1,#REF!)", true),
            CellValue::Error(CellError::Ref)
        );
    }
}

#[test]
fn excel_serial_weekdays_and_iso_week_years_are_explicit() {
    // Microsoft documents Excel's pre-March-1900 WEEKDAY defect and each
    // return-type origin. Calc 26.2.6.3 independently agrees on these serials.
    // https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/wrongly-assumes-1900-is-leap-year
    // https://support.microsoft.com/en-us/excel/functions/weekday-function
    let modes = [
        (1, [1, 2, 3, 4, 5, 6, 7]),
        (2, [7, 1, 2, 3, 4, 5, 6]),
        (3, [6, 0, 1, 2, 3, 4, 5]),
        (11, [7, 1, 2, 3, 4, 5, 6]),
        (12, [6, 7, 1, 2, 3, 4, 5]),
        (13, [5, 6, 7, 1, 2, 3, 4]),
        (14, [4, 5, 6, 7, 1, 2, 3]),
        (15, [3, 4, 5, 6, 7, 1, 2]),
        (16, [2, 3, 4, 5, 6, 7, 1]),
        (17, [1, 2, 3, 4, 5, 6, 7]),
    ];
    for (system, excel_days) in [
        (DateSystem::Excel1900, [7, 1, 3, 4, 5]),
        (DateSystem::Excel1904, [6, 7, 2, 3, 4]),
    ] {
        let w = book(system);
        for (serial, excel_day) in [0, 1, 59, 60, 61].into_iter().zip(excel_days) {
            for (mode, outputs) in modes {
                // Also exercise fractional serial truncation, preserving the day.
                for suffix in ["", ".99"] {
                    let formula = format!("WEEKDAY({serial}{suffix},{mode})");
                    assert_eq!(
                        value(&w, &formula, true),
                        CellValue::Number(outputs[excel_day - 1] as f64),
                        "{system:?}: {formula}"
                    );
                }
            }
        }
        for formula in [
            "WEEKDAY(-1)",
            "WEEKDAY(1,0)",
            "WEEKDAY(1,4)",
            "WEEKDAY(1,18)",
        ] {
            assert_eq!(value(&w, formula, true), CellValue::Error(CellError::Num));
        }
        assert_eq!(
            value(&w, "WEEKDAY(#REF!)", true),
            CellValue::Error(CellError::Ref)
        );
        assert_eq!(
            value(&w, "WEEKDAY(1,#REF!)", true),
            CellValue::Error(CellError::Ref)
        );
    }
}

#[test]
fn iso_boundaries_cover_every_leap_and_january_weekday_family() {
    // Each row contains Dec 29, 30, 31 and Jan 1, 2, 3, 4 of the next year.
    // Python datetime.date.isocalendar supplied these fixed independent values;
    // Calc 26.2.6.3 corroborated all 196 checks across both workbook epochs.
    // The first-Thursday definition is Microsoft's WEEKNUM system 2 contract:
    // https://support.microsoft.com/en-us/excel/functions/weeknum-function
    let families = [
        (2001, [52, 52, 1, 1, 1, 1, 1]), // Common, Monday Jan 1.
        (2002, [52, 1, 1, 1, 1, 1, 1]),
        (2003, [1, 1, 1, 1, 1, 1, 1]),
        (2009, [53, 53, 53, 53, 53, 53, 1]),
        (2010, [52, 52, 52, 52, 52, 1, 1]),
        (2005, [52, 52, 52, 52, 1, 1, 1]),
        (2006, [52, 52, 52, 1, 1, 1, 1]),
        (2024, [52, 1, 1, 1, 1, 1, 1]), // Leap, Monday Jan 1.
        (2008, [1, 1, 1, 1, 1, 1, 1]),
        (2020, [53, 53, 53, 53, 53, 53, 1]),
        (2004, [53, 53, 53, 53, 53, 1, 1]),
        (2016, [52, 52, 52, 52, 1, 1, 1]),
        (2000, [52, 52, 52, 1, 1, 1, 1]),
        (2012, [52, 52, 1, 1, 1, 1, 1]),
    ];
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for (year, weeks) in families {
            for (offset, expected) in weeks.into_iter().enumerate() {
                let (year, month, day) = if offset < 3 {
                    (year, 12, 29 + offset)
                } else {
                    (year + 1, 1, offset - 2)
                };
                let formula = format!("WEEKNUM(DATE({year},{month},{day}),21)");
                assert_eq!(
                    value(&w, &formula, true),
                    CellValue::Number(expected as f64),
                    "{system:?}: {formula}"
                );
            }
        }
    }
}

#[test]
fn system_one_week_modes_keep_calendar_year() {
    // System 1 numbers the week containing Jan 1 as week 1 and stays in that
    // calendar year; every weekday origin is documented by Microsoft. Calc
    // 26.2.6.3 instead rolls most of these Dec 31 modes to 1, so its result
    // cannot establish these expectations. The Thursday-start leap-year
    // calendar has both a one-day first week and a one-day week 54.
    // https://support.microsoft.com/en-us/excel/functions/weeknum-function
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for mode in [1, 2, 11, 12, 13, 14, 15, 16, 17] {
            let expected = if mode == 14 { 54. } else { 53. };
            let formula = format!("WEEKNUM(DATE(2020,12,31),{mode})");
            assert_eq!(value(&w, &formula, true), CellValue::Number(expected));
            let formula = format!("WEEKNUM(DATE(2021,1,1),{mode})");
            assert_eq!(value(&w, &formula, true), CellValue::Number(1.));
        }
        assert_eq!(
            value(&w, "WEEKNUM(1,3)", true),
            CellValue::Error(CellError::Num)
        );
    }
}

#[test]
fn early_week_modes_preserve_the_existing_civil_calendar_contract() {
    // Fixed values from Python datetime's Gregorian dates and ISO calendar.
    // Keep this explicit consumer contract separate from Excel WEEKDAY's serial
    // cycle and Calc's earlier civil-date origin. Serial 60 maps to March 1
    // in the existing calculation calendar, just as serial 61 does.
    let modes = [1, 2, 11, 12, 13, 14, 15, 16, 17, 21];
    for (system, cases) in [
        (
            DateSystem::Excel1900,
            [
                (0, [53, 53, 53, 53, 53, 53, 53, 53, 53, 52]),
                (1, [1, 1, 1, 1, 1, 1, 1, 1, 1, 1]),
                (59, [9, 9, 9, 10, 10, 9, 9, 9, 9, 9]),
                (60, [9, 9, 9, 10, 10, 10, 9, 9, 9, 9]),
                (61, [9, 9, 9, 10, 10, 10, 9, 9, 9, 9]),
            ],
        ),
        (
            DateSystem::Excel1904,
            [
                (0, [1, 1, 1, 1, 1, 1, 1, 1, 1, 53]),
                (1, [1, 1, 1, 1, 1, 1, 1, 2, 1, 53]),
                (59, [10, 10, 10, 9, 9, 9, 9, 10, 10, 9]),
                (60, [10, 10, 10, 10, 9, 9, 9, 10, 10, 9]),
                (61, [10, 10, 10, 10, 10, 9, 9, 10, 10, 9]),
            ],
        ),
    ] {
        let w = book(system);
        for (serial, expected) in cases {
            for (mode, week) in modes.into_iter().zip(expected) {
                let formula = format!("WEEKNUM({serial},{mode})");
                assert_eq!(
                    value(&w, &formula, true),
                    CellValue::Number(week as f64),
                    "{system:?}: {formula}"
                );
            }
        }
    }
}

#[test]
fn date_argument_requests_preserve_context_and_error_precedence() {
    // ClosedXML's pinned date-function assertions and fresh Calc OOXML probes
    // independently establish these admission results. Modern numeric arguments
    // share their numeric and calendar continuation.
    let mut w = book(DateSystem::Excel1900);
    w.sheet_mut(0)
        .unwrap()
        .set_value("B1".parse().unwrap(), "1/2/2020")
        .unwrap();
    for (formula, expected) in [
        ("VALUE(\"1/2/2020\")", 43832.),
        ("ABS(\"01/02/2020\")", 43832.),
        ("\"2020-01-02\"*1", 43832.),
        ("VALUE(B1)", 43832.),
        ("ABS(INDEX(B1:B1,1))", 43832.),
        ("VALUE(\"1/2/2020 12:00\")", 43832.5),
        (
            "VALUE(\"1/2/2020 12:00:00.500\")",
            43832. + (43200. + 0.5) / 86400.,
        ),
        ("VALUE(\"1/2/2020 24:00\")", 43833.),
        ("EDATE(\"2008-03-31\",-1)", 39507.),
        ("EDATE(\"1983-07-31\",-77)", 28184.),
        ("EDATE(\"1/2/2020\",\"1 1/2\")", 43863.),
        ("EDATE(TRUE,1)", 32.),
        ("EDATE(1,TRUE)", 32.),
        ("WEEKDAY(\"1/2/2020\")", 5.),
        ("WEEKDAY(\"0 2/2\")", 1.),
        ("WEEKDAY(1,\"1 0/2\")", 1.),
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Number(expected),
            "{formula}"
        );
    }
    for (formula, expected) in [
        ("VALUE(\"2/30/2020\")", CellError::Value),
        ("VALUE(\"1/2/2020 garbage\")", CellError::Value),
        ("WEEKDAY(1,\"text\")", CellError::Value),
        ("WEEKDAY(5,#N/A)", CellError::NA),
        ("WEEKDAY(#REF!,#N/A)", CellError::Ref),
        ("WEEKDAY(\"bad\",#N/A)", CellError::Value),
        ("EDATE(#REF!,#N/A)", CellError::Ref),
        ("WEEKDAY(1,)", CellError::Num),
        ("WEEKDAY(1,C1)", CellError::Num),
        ("WEEKDAY(1,18)", CellError::Num),
        ("WEEKDAY(1,1e20)", CellError::Num),
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Error(expected),
            "{formula}"
        );
    }
    let w = book(DateSystem::Excel1904);
    assert_eq!(
        value(&w, "VALUE(\"1/2/2020\")", true),
        CellValue::Number(42370.)
    );
    assert!(matches!(
        run(
            &w,
            "IFERROR(VALUE(\"1/2/2020\"),7)",
            true,
            EvaluationBudgets {
                text_bytes: 3,
                ..Default::default()
            }
        ),
        EvaluationOutcome::Aborted { .. }
    ));
}

#[test]
fn business_days_apply_signed_endpoints_and_holiday_rules() {
    // Microsoft WORKDAY.INTL documents truncation, signed offsets, mask order,
    // holiday bounds, and zero offset. ClosedXML's pinned assertions and Calc
    // independently corroborate the serial cycle and inclusive signed counts.
    // See poirot-test-runner/evidence/business-days-contract.md.
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let w = book(system);
        for (formula, expected) in [
            ("NETWORKDAYS(45000,45006)", 5.),
            ("NETWORKDAYS(45006,45000)", -5.),
            ("NETWORKDAYS(45000,45006,{45000,45000,44999,45007})", 4.),
            ("NETWORKDAYS(45006,45000,{45000,45000,44999,45007})", -4.),
            ("WORKDAY(45000,0)", 45000.),
            ("WORKDAY(45000.9,0)", 45000.),
            ("WORKDAY.INTL(45000,5,\"0000000\")", 45005.),
            ("WORKDAY.INTL(0,1,\"0000000\",TRUE)", 2.),
            ("WORKDAY.INTL(0,1,\"0000000\",{TRUE})", 2.),
            ("WORKDAY.INTL(1,-1,\"0000000\",B10)", 0.),
            ("WORKDAY.INTL(45000,-5,\"0000000\")", 44995.),
            (
                "WORKDAY.INTL(45000,1,\"0000000\",{45001.1,45001.9})",
                45002.,
            ),
            ("WORKDAY.INTL(45000,1,\"0000000\",{TRUE,FALSE})", 45001.),
        ] {
            assert_eq!(
                value(&w, formula, true),
                CellValue::Number(expected),
                "{system:?}: {formula}"
            );
        }
        let early_shifts = if system == DateSystem::Excel1900 {
            [
                (0, 1, 2.),
                (1, 1, 2.),
                (59, -1, 58.),
                (59, 1, 60.),
                (60, -1, 59.),
                (60, 1, 61.),
                (61, -1, 60.),
                (61, 1, 62.),
            ]
        } else {
            [
                (0, 1, 3.),
                (1, 1, 3.),
                (59, -1, 56.),
                (59, 1, 60.),
                (60, -1, 59.),
                (60, 1, 61.),
                (61, -1, 60.),
                (61, 1, 62.),
            ]
        };
        for (start, shift, expected) in early_shifts {
            assert_eq!(
                value(&w, &format!("WORKDAY({start},{shift})"), true),
                CellValue::Number(expected)
            );
        }
        let upper = if system == DateSystem::Excel1900 {
            2958465
        } else {
            2957003
        };
        for formula in [
            format!("WORKDAY({upper},1)"),
            "WORKDAY(0,-1)".into(),
            format!("WORKDAY(1,0,{{{}}})", upper + 1),
            "NETWORKDAYS(1,10,{-1})".into(),
            "WORKDAY.INTL(1,-1,\"0000000\",)".into(),
        ] {
            assert_eq!(
                value(&w, &formula, true),
                CellValue::Error(CellError::Num),
                "{formula}"
            );
        }
        for formula in [
            "WORKDAY.INTL(1,0,\"1111111\")",
            "WORKDAY.INTL(1,1,\"001\")",
            "WORKDAY.INTL(1,1,\"000000x\")",
        ] {
            assert_eq!(value(&w, formula, true), CellValue::Error(CellError::Value));
        }
        for (n, weekday) in if system == DateSystem::Excel1900 {
            [(0, 6), (1, 0), (59, 2), (60, 3), (61, 4)]
        } else {
            [(0, 5), (1, 6), (59, 1), (60, 2), (61, 3)]
        } {
            let expected = f64::from(![0, 6].contains(&weekday));
            assert_eq!(
                value(&w, &format!("NETWORKDAYS({n},{n})"), true),
                CellValue::Number(expected)
            );
            assert_eq!(
                value(&w, &format!("WORKDAY({n},0)"), true),
                CellValue::Number(n as f64)
            );
            assert_eq!(
                value(&w, &format!("NETWORKDAYS({n},{n},{{{n},{n}}})"), true),
                CellValue::Number(0.)
            );
        }
    }
    let w = book(DateSystem::Excel1900);
    for (formula, expected) in [
        ("WORKDAY(0,1)", 2.),
        ("WORKDAY(59,1)", 60.),
        ("WORKDAY(60,1)", 61.),
        ("WORKDAY(61,-1)", 60.),
        ("WORKDAY(10,-1.5)", 9.),
        ("WORKDAY(2.9,1)", 3.),
        ("NETWORKDAYS(1,10,{2,2,3})", 5.),
        ("NETWORKDAYS(10,1,{2,2,3})", -5.),
        ("NETWORKDAYS(2.9,3.1,{2.1,2.9})", 1.),
        ("WORKDAY(1,1,{\"2\",3})", 4.),
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Number(expected),
            "{formula}"
        );
    }
}

#[test]
fn business_day_holidays_retain_origin_errors_and_resource_failures() {
    let mut w = book(DateSystem::Excel1900);
    for (row, v) in [
        (0, CellValue::Text("2".into())),
        (1, CellValue::Text("bad".into())),
        (2, CellValue::Boolean(true)),
        (3, CellValue::Error(CellError::Ref)),
        (4, CellValue::Number(2.)),
    ] {
        w.sheet_mut(0)
            .unwrap()
            .set_value(CellAddress { row, col: 1 }, v)
            .unwrap();
    }
    for (formula, expected) in [
        ("NETWORKDAYS(1,10,B1:B3)", CellValue::Number(7.)),
        ("WORKDAY(1,1,B1)", CellValue::Number(3.)),
        ("WORKDAY(1,1,B2)", CellValue::Error(CellError::Value)),
        ("NETWORKDAYS(1,10,B4)", CellValue::Error(CellError::Ref)),
        ("WORKDAY(1,1,B4)", CellValue::Error(CellError::Ref)),
        ("NETWORKDAYS(1,10,B5)", CellValue::Number(6.)),
        ("WORKDAY(1,0,{#REF!})", CellValue::Error(CellError::Ref)),
    ] {
        assert_eq!(value(&w, formula, true), expected, "{formula}");
    }
    let result = run(
        &w,
        "WORKDAY.INTL(1,1000000,\"0000000\")",
        true,
        EvaluationBudgets::default(),
    );
    match result {
        EvaluationOutcome::Complete { value, usage, .. } => {
            assert_eq!(value, CellValue::Number(1000001.));
            assert!(usage.calendar_steps <= 8);
        }
        other => panic!("{other:?}"),
    }
    let budgets = EvaluationBudgets {
        run_work: 0,
        ..EvaluationBudgets::default()
    };
    assert!(!matches!(
        run(&w, "WORKDAY(1,1000000)", true, budgets),
        EvaluationOutcome::Complete { .. }
    ));
}

#[test]
fn datedif_preserves_reference_inputs_policies_and_operational_limits() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let mut w = book(system);
        w.sheet_mut(0)
            .unwrap()
            .set_value("B1".parse().unwrap(), "2020-01-31")
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("B2".parse().unwrap(), "2024-03-01")
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("B3".parse().unwrap(), "md")
            .unwrap();
        {
            let excel = true;
            assert_eq!(
                value(&w, "DATEDIF(B1,B2,B3)", excel),
                CellValue::Number(-1.)
            );
            assert_eq!(
                value(&w, "DATEDIF(A7,A8,\"D\")", excel),
                CellValue::Error(CellError::Num)
            );
            assert_eq!(
                value(&w, "DATEDIF(1,2,\"n\")", excel),
                CellValue::Error(CellError::Num)
            );
            for formula in ["IFERROR(DATEDIF(B1,B2,B3),7)", "IFERROR(DATEDIF(1,2,B3),7)"] {
                assert!(matches!(
                    run(
                        &w,
                        formula,
                        excel,
                        EvaluationBudgets {
                            text_bytes: 1,
                            ..Default::default()
                        }
                    ),
                    EvaluationOutcome::Aborted { .. }
                ));
            }
            assert!(matches!(
                run(
                    &w,
                    "IFERROR(DATEDIF(B1,B2,B3),7)",
                    excel,
                    EvaluationBudgets {
                        run_work: 1,
                        ..Default::default()
                    }
                ),
                EvaluationOutcome::Aborted { .. }
            ));
        }
    }
}
