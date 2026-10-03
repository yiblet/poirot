use poirot_eval::{
    BuiltinSemanticsPolicy, EvaluationBudgets, EvaluationOutcome, RunConfiguration,
    run_formula_with_configuration,
};
use poirot_model::{CellAddress, CellError, CellValue, DateSystem, Workbook};

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
fn date_serial_overflow_and_errors_preserve_pinned_contract() {
    let w = book(DateSystem::Excel1900);
    for (formula, excel, pinned) in [
        ("DATE(1900,1,0)", 0., 0.),
        ("DATE(1900,1,60)", 60., 59.),
        ("DATE(1900,2,29)", 60., 60.),
        ("DATE(1900,3,0)", 60., 59.),
        ("DATE(0,3,0)", 60., 59.),
        ("DATE(A1,A2,A3)", 60., 59.),
        ("DATE(1900,3,-1)", 59., 59.),
        ("DATE(1900,2,30)", 61., 61.),
        ("DATE(1900,0,32)", 1., 1.),
        ("DATE(1900,0.9,32)", 1., 32.),
        ("DATE(2024,14,0)", 45688., 45688.),
        ("DATE(9999,12,31)", 2958465., 2958465.),
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Number(excel),
            "{formula}"
        );
        assert_eq!(
            value(&w, formula, false),
            CellValue::Number(pinned),
            "{formula}"
        );
    }
    for formula in [
        "DATE(-1,1,1)",
        "DATE(10000,1,1)",
        "DATE(10000,0,1)",
        "DATE(9999,12,32)",
        "DATE(1900,1,-1)",
    ] {
        assert_eq!(
            value(&w, formula, true),
            CellValue::Error(CellError::Num),
            "{formula}"
        );
    }
    assert_eq!(
        value(&w, "DATE(-1,1,1)", false),
        CellValue::Error(CellError::Value)
    );
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
    assert_eq!(value(&w, "DAYS(A7,A8)", false), CellValue::Number(2.));
    assert_eq!(value(&w, "DAYS(61,60)", false), CellValue::Number(0.));
    assert_eq!(
        value(&w, "WORKDAY(A5,-1.9)", false),
        CellValue::Number(45350.)
    );
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
            value(&w, "WEEKNUM(DATE(2021,1,1),21)", false),
            CellValue::Number(0.)
        );
        assert_eq!(
            value(&w, "WEEKNUM(1,18)", true),
            CellValue::Error(CellError::Num)
        );
        assert_eq!(
            value(&w, "WEEKNUM(1,#REF!)", true),
            CellValue::Error(CellError::Num)
        );
    }
}
