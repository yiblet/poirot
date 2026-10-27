//! TEXT value admission reuses contextual numeric parsing without changing the renderer.
use poirot::eval::overlay::Overlay;
use poirot::eval::*;
use poirot::model::{CellError, CellValue, ExcelText, Workbook};

fn target(address: &str) -> EvaluationTarget {
    EvaluationTarget {
        sheet: 0,
        address: address.parse().unwrap(),
    }
}
fn excel() -> RunConfiguration {
    RunConfiguration::default()
}
fn value(book: &Workbook, formula: &str, policy: &RunConfiguration) -> CellValue {
    match run_formula_with_configuration(
        book,
        0,
        target("Z1").address,
        formula,
        policy,
        Default::default(),
    ) {
        EvaluationOutcome::Complete { value, .. } => value,
        other => panic!("{formula}: {other:?}"),
    }
}

#[test]
fn text_admits_contextual_numeric_text_before_applying_the_requested_format() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    // Independently corroborated with typed Calc results in text-format-admission-probes.json.
    for (input, pattern, expected) in [
        ("15:43:09", "H:MM:ss AM/PM", "3:43:09 PM"),
        ("00:00:00", "h:mm AM/PM", "12:00 AM"),
        ("12:00:00", "h:mm AM/PM", "12:00 PM"),
        ("15:43:09", "hh:mm:ss", "15:43:09"),
        ("1.25", "0.00", "1.25"),
        ("$1,234.50", "0.00", "1234.50"),
        ("12.5%", "0.000", "0.125"),
        ("1/2/2024", "yyyy-mm-dd", "2024-01-02"),
        (
            "1/2/2024 15:43:09",
            "yyyy-mm-dd h:mm:ss AM/PM",
            "2024-01-02 3:43:09 PM",
        ),
        ("(123.4)", "0.00", "-123.40"),
        ("1.25E2", "0.00", "125.00"),
    ] {
        assert_eq!(
            value(&book, &format!(r#"TEXT("{input}","{pattern}")"#), &excel()),
            CellValue::Text(expected.into())
        );
        for source in [
            CellValue::Text(input.into()),
            CellValue::TextUtf16(ExcelText::from_units(
                input.encode_utf16().collect::<Vec<_>>(),
            )),
        ] {
            book.sheet_mut(0)
                .unwrap()
                .set_value(target("A1").address, source)
                .unwrap();
            assert_eq!(
                value(&book, &format!(r#"TEXT(A1,"{pattern}")"#), &excel()),
                CellValue::Text(expected.into()),
                "{input}"
            );
        }
    }
    // The number argument and renderer already support this pattern independently of parsing.
    assert_eq!(
        value(
            &book,
            r#"TEXT(0.6549652777777778,"H:MM:ss AM/PM")"#,
            &excel()
        ),
        "3:43:09 PM".into()
    );
}

#[test]
fn rejected_text_and_other_value_classes_keep_their_existing_text_contract() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    for config in [excel(), RunConfiguration::default()] {
        for input in ["word", "1..25", "", "  ", "25:61:00", "NaN", "TRUE"] {
            for stored in [
                CellValue::Text(input.into()),
                CellValue::TextUtf16(ExcelText::from_units(
                    input.encode_utf16().collect::<Vec<_>>(),
                )),
            ] {
                book.sheet_mut(0)
                    .unwrap()
                    .set_value(target("A1").address, stored.clone())
                    .unwrap();
                assert_eq!(value(&book, r#"TEXT(A1,"0.00")"#, &config), stored);
            }
        }
        // Deliberately preserve the established boolean contract; Calc converts these to 1/0.
        assert_eq!(value(&book, r#"TEXT(TRUE,"0.00")"#, &config), "TRUE".into());
        assert_eq!(
            value(&book, r#"TEXT(FALSE,"0.00")"#, &config),
            "FALSE".into()
        );
        assert_eq!(value(&book, r#"TEXT(Z9,"0.00")"#, &config), "0.00".into());
        assert_eq!(
            value(&book, r#"TEXT(1/0,"0.00")"#, &config),
            CellValue::Error(CellError::Div0)
        );
        let malformed = CellValue::TextUtf16(ExcelText::from_units(vec![0xd800, 65]));
        book.sheet_mut(0)
            .unwrap()
            .set_value(target("A1").address, malformed.clone())
            .unwrap();
        assert_eq!(value(&book, r#"TEXT(A1,"0.00")"#, &config), malformed);
    }
}

#[test]
fn contextual_text_admission_tracks_locale_calendar_profiles_edits_and_snapshots() {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("S").unwrap();
    for (address, input) in [("A1", "1,234"), ("A2", "1/2/2024"), ("A3", "15:43:09")] {
        sheet.set_value(target(address).address, input).unwrap();
    }
    for (address, formula) in [
        ("B1", r#"TEXT(A1,"0.000")"#),
        ("B2", r#"TEXT(A2,"yyyy-mm-dd")"#),
        ("B3", r#"TEXT(A3,"h:mm:ss AM/PM")"#),
        ("C1", r#"B1&"!""#),
    ] {
        sheet.set_formula(target(address).address, formula).unwrap();
    }
    let mut overlay = Overlay::with_workbook(&book)
        .with_policy(EvaluationPolicy {
            ..Default::default()
        })
        .build()
        .unwrap();
    let mut context = EvaluationContext::default();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B3"))
            .unwrap(),
        "3:43:09 PM".into()
    );
    overlay
        .set_policy(EvaluationPolicy {
            ..Default::default()
        })
        .unwrap();
    for (address, expected) in [
        ("B1", "1234.000"),
        ("B2", "2024-01-02"),
        ("B3", "3:43:09 PM"),
        ("C1", "1234.000!"),
    ] {
        assert_eq!(
            overlay
                .evaluation(&context)
                .unwrap()
                .cell(target(address))
                .unwrap(),
            CellValue::Text(expected.into())
        );
    }
    context.number_locale =
        poirot_parsing::number::NumberLocale::new(",", Some("."), Some("€"), "%", ":").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("C1"))
            .unwrap(),
        "1.234!".into()
    );
    context.localization.date_order = DateOrder::DayMonthYear;
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        "2024-02-01".into()
    );
    context.localization.date_order = DateOrder::MonthDayYear;
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B2"))
            .unwrap(),
        "2024-01-02".into()
    );
    let (restored, state) = Overlay::from_snapshot(overlay.snapshot()).unwrap();
    let mut overlay = state.attach(&restored).unwrap();
    overlay.set_value(target("A3"), "00:00:00").unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B3"))
            .unwrap(),
        "12:00:00 AM".into()
    );
    overlay
        .set_policy(EvaluationPolicy {
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B3"))
            .unwrap(),
        "12:00:00 AM".into()
    );
    overlay
        .set_policy(EvaluationPolicy {
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        overlay
            .evaluation(&context)
            .unwrap()
            .cell(target("B3"))
            .unwrap(),
        "12:00:00 AM".into()
    );
}

#[test]
fn text_calendar_admission_uses_workbook_epoch_and_does_not_swallow_operational_limits() {
    let mut book = Workbook::new();
    book.add_sheet("S").unwrap();
    assert_eq!(
        value(&book, r#"TEXT("1/1/1904","0")"#, &excel()),
        "1462".into()
    );
    book.date_system = poirot::model::DateSystem::Excel1904;
    assert_eq!(
        value(&book, r#"TEXT("1/1/1904","0")"#, &excel()),
        "0".into()
    );
    let syntax =
        poirot_parsing::formula::parse_formula(r#"IFERROR(TEXT("15:43:09","h:mm:ss AM/PM"),99)"#)
            .unwrap();
    let plan = poirot::eval::compile::bound::FormulaPlan::bind(&syntax).unwrap();
    for budget in [
        EvaluationBudgets {
            text_bytes: 4,
            ..Default::default()
        },
        EvaluationBudgets {
            run_work: 10,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            run_plan_with_configuration(&book, 0, target("Z1").address, &plan, &excel(), budget),
            EvaluationOutcome::Aborted { .. }
        ));
    }
}
