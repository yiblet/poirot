use super::*;

#[test]
fn timevalue_dispatch_preserves_source_cancellation_and_permissive_date_fallback() {
    for system in [DateSystem::Excel1900, DateSystem::Excel1904] {
        let mut workbook = Workbook::new();
        workbook.date_system = system;
        workbook.add_sheet("Sheet1").unwrap();
        for (formula, expected) in [
            ("TIMEVALUE(\"12:30:15\")", Number(0.5210069444437977)),
            ("TIMEVALUE(\"12:03:45.386\")", Number(0.5026041666642413)),
            (
                "TIMEVALUE(\"1/01/2000 12:30:15\")",
                Number(0.5210069444437977),
            ),
            ("TIMEVALUE(\"1/01/2000 trailing text\")", Number(0.)),
            ("TIMEVALUE(\"2/29/2000\")", Number(0.)),
            ("TIMEVALUE(\"2/29/1900 12:00\")", Error(VALUE_ERROR)),
            ("TIMEVALUE(\"bogus 12:00\")", Error(VALUE_ERROR)),
            ("TIMEVALUE(\"24:00\")", Number(1.)),
            // Public evaluation retains the blank returned by this parser.
            ("TIMEVALUE(\"\")", Blank),
            ("TIMEVALUE(FALSE)", Error(VALUE_ERROR)),
            ("TIMEVALUE(2.718281828459045)", Error(VALUE_ERROR)),
        ] {
            let actual = match crate::run_formula(
                &workbook,
                0,
                "P1".parse().unwrap(),
                formula,
                EvaluationInputs {
                    as_of_time: UNIX_EPOCH + Duration::from_secs(1700000000),
                    randomness_seed: 7,
                    localization: Localization {
                        utc_offset_seconds: 0,
                        date_order: DateOrder::MonthDayYear,
                    },
                },
                Default::default(),
            ) {
                crate::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            };
            match (&actual, &expected) {
                (Number(actual), Number(expected)) => {
                    assert_eq!(
                        actual.to_bits(),
                        expected.to_bits(),
                        "{system:?}: {formula}"
                    );
                }
                _ => assert_eq!(actual, expected, "{system:?}: {formula}"),
            }
        }
    }
}

#[test]
fn timevalue_empty_remains_blank_through_public_evaluation() {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    assert_eq!(
        match crate::run_formula(
            &workbook,
            0,
            "A1".parse().unwrap(),
            "TIMEVALUE(\"\")",
            EvaluationInputs::default(),
            Default::default()
        ) {
            crate::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        },
        Blank
    );
}
