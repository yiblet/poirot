use super::*;
fn run(
    function: DatabaseFunction,
    data: &[CellValue],
    columns: usize,
    field: CellValue,
    criteria: &[CellValue],
    ccols: usize,
) -> Result<CellValue, DatabaseError> {
    DatabasePlan::new(
        function,
        Grid::new(data.len() / columns, columns, data).unwrap(),
        &field,
        Grid::new(criteria.len() / ccols, ccols, criteria).unwrap(),
    )?
    .execute()
}
// DStarRunner AND columns, OR rows, blank criteria; algorithm NumericValueEval.
#[test]
fn all_aggregators_filter_the_same_rows() {
    let data = [
        "Kind".into(),
        "Amount".into(),
        "Apple".into(),
        2.0.into(),
        "Apricot".into(),
        4.0.into(),
        "Pear".into(),
        99.0.into(),
    ];
    let criteria = ["Kind".into(), "Ap".into()];
    use DatabaseFunction::*;
    for (function, expected) in [
        (Sum, 6.),
        (Average, 3.),
        (Count, 2.),
        (CountA, 2.),
        (Min, 2.),
        (Max, 4.),
        (Product, 8.),
        (Var, 2.),
        (VarP, 1.),
        (Stdev, 2f64.sqrt()),
        (StdevP, 1.),
    ] {
        assert_eq!(
            run(function, &data, 2, "amount".into(), &criteria, 1),
            Ok(expected.into())
        );
    }
    assert_eq!(
        run(Get, &data, 2, "Amount".into(), &criteria, 1),
        Ok(CellValue::Error(CellError::Num))
    );
}
#[test]
fn boolean_numeric_and_reference_text_blank_error_rules() {
    let data = [
        "N".into(),
        true.into(),
        false.into(),
        "7".into(),
        CellValue::Blank,
        CellValue::Error(CellError::NA),
        "".into(),
    ];
    let criteria = ["N".into(), CellValue::Blank];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 1, 1.0.into(), &criteria, 1),
        Ok(1.0.into())
    );
    assert_eq!(
        run(DatabaseFunction::Count, &data, 1, 1.0.into(), &criteria, 1),
        Ok(2.0.into())
    );
    assert_eq!(
        run(DatabaseFunction::CountA, &data, 1, 1.0.into(), &criteria, 1),
        Ok(5.0.into())
    );
    assert_eq!(
        run(
            DatabaseFunction::Count,
            &data,
            1,
            CellValue::Error(CellError::Ref),
            &criteria,
            1
        ),
        Ok(6.0.into())
    );
}
#[test]
fn get_ignores_blank_matches_but_empty_text_is_a_filled_match() {
    let criteria = ["N".into(), CellValue::Blank];
    for data in [
        vec!["N".into(), CellValue::Blank, 5.0.into(), CellValue::Blank],
        vec!["N".into(), CellValue::Blank, 5.0.into()],
    ] {
        assert_eq!(
            run(DatabaseFunction::Get, &data, 1, 1.0.into(), &criteria, 1),
            Ok(5.0.into())
        );
    }
    let data = ["N".into(), "".into(), 5.0.into()];
    assert_eq!(
        run(DatabaseFunction::Get, &data, 1, 1.0.into(), &criteria, 1),
        Ok(CellValue::Error(CellError::Num))
    );
}
#[test]
fn empty_result_contracts() {
    let data = ["N".into(), 2.0.into()];
    let criteria = ["N".into(), ">5".into()];
    for function in [
        DatabaseFunction::Sum,
        DatabaseFunction::Average,
        DatabaseFunction::Product,
        DatabaseFunction::Min,
        DatabaseFunction::Max,
        DatabaseFunction::Count,
        DatabaseFunction::CountA,
    ] {
        assert_eq!(
            run(function, &data, 1, 1.0.into(), &criteria, 1),
            Ok(0.0.into())
        );
    }
    for function in [
        DatabaseFunction::Var,
        DatabaseFunction::VarP,
        DatabaseFunction::Stdev,
        DatabaseFunction::StdevP,
    ] {
        assert_eq!(
            run(function, &data, 1, 1.0.into(), &criteria, 1),
            Ok(CellValue::Error(CellError::Div0))
        );
    }
    assert_eq!(
        run(DatabaseFunction::Get, &data, 1, 1.0.into(), &criteria, 1),
        Ok(CellValue::Error(CellError::Value))
    );
}
#[test]
fn criteria_have_database_specific_matching_rules() {
    assert_eq!(condition_matches(&"12".into(), &12.0.into()), Ok(true));
    assert_eq!(condition_matches(&"12".into(), &"=12".into()), Ok(false));
    assert_eq!(condition_matches(&"Apricot".into(), &"Ap".into()), Ok(true));
    assert_eq!(
        condition_matches(&"Apricot".into(), &"=Ap".into()),
        Ok(false)
    );
    assert_eq!(
        condition_matches(&"Apricot".into(), &"Ap*".into()),
        Ok(true)
    );
    assert_eq!(
        condition_matches(&"Apricot".into(), &"=Ap*".into()),
        Ok(false)
    );
    assert_eq!(condition_matches(&CellValue::Blank, &"=".into()), Ok(true));
    assert_eq!(condition_matches(&"".into(), &"=".into()), Ok(false));
    assert_eq!(condition_matches(&"".into(), &"".into()), Ok(true));
    assert_eq!(condition_matches(&CellValue::Blank, &"".into()), Ok(false));
    assert_eq!(
        condition_matches(&2.0.into(), &">bad".into()),
        Err(DatabaseError::Cell(CellError::Value))
    );
    // POI checks target numeric type before parsing malformed numeric operator.
    assert_eq!(condition_matches(&"text".into(), &">bad".into()), Ok(false));
}
#[test]
fn and_or_and_short_circuit_unsupported_headers() {
    let data = [
        "N".into(),
        "Tag".into(),
        2.0.into(),
        "A".into(),
        4.0.into(),
        "B".into(),
        6.0.into(),
        "A".into(),
    ];
    let criteria = [
        "N".into(),
        "Tag".into(),
        ">3".into(),
        "A".into(),
        "=2".into(),
        "B".into(),
    ];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, 1.0.into(), &criteria, 2),
        Ok(6.0.into())
    );
    let criteria = ["N".into(), "Unknown".into(), ">10".into(), true.into()];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, 1.0.into(), &criteria, 2),
        Ok(0.0.into())
    );
    let criteria = ["Unknown".into(), true.into()];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, 1.0.into(), &criteria, 1),
        Err(DatabaseError::FormulaCriteria)
    );
}
#[test]
fn numeric_criteria_headers_are_positional_and_min_max_retain_type() {
    let data = ["N".into(), true.into(), false.into()];
    let criteria = [1.9.into(), CellValue::Blank];
    assert_eq!(
        run(DatabaseFunction::Min, &data, 1, 1.0.into(), &criteria, 1),
        Ok(false.into())
    );
    assert_eq!(
        run(DatabaseFunction::Max, &data, 1, 1.0.into(), &criteria, 1),
        Ok(true.into())
    );
    let criteria = [1.9.into(), ">0".into()];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 1, 1.0.into(), &criteria, 1),
        Ok(1.0.into())
    );
}
#[test]
fn java_numeric_criteria_syntax() {
    assert_eq!(
        condition_matches(&" 0x1.8p1D ".into(), &3.0.into()),
        Ok(true)
    );
    assert_eq!(condition_matches(&3.0.into(), &"=0x1.8p1".into()), Ok(true));
    assert_eq!(condition_matches(&3.0.into(), &"=3f".into()), Ok(true));
    assert_eq!(coerce_java_number_text("inf"), None);
    assert_eq!(coerce_java_number_text("3\u{a0}"), None);
}
#[test]
fn shapes_headers_and_rounding() {
    assert!(Grid::new(0, 1, &[]).is_err());
    assert!(Grid::new(2, 2, &[CellValue::Blank]).is_err());
    let data = [
        CellValue::Error(CellError::NA),
        "V".into(),
        1.0.into(),
        9.0.into(),
    ];
    let criteria = ["V".into(), CellValue::Blank];
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, 1.5.into(), &criteria, 1),
        Ok(9.0.into())
    );
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, "v".into(), &criteria, 1),
        Ok(9.0.into())
    );
    assert_eq!(
        run(DatabaseFunction::Sum, &data, 2, 3.0.into(), &criteria, 1),
        Err(DatabaseError::Cell(CellError::Value))
    );
}

// Empty means no admitted numeric observations, rather than a zero-valued sum.
#[test]
fn database_average_distinguishes_empty_samples_from_zero_values() {
    for (values, condition, empty) in [
        (vec![], CellValue::Blank, true),
        (vec![2.0.into()], ">5".into(), true),
        (
            vec!["text".into(), CellValue::Blank, "0".into()],
            CellValue::Blank,
            true,
        ),
        (vec![0.0.into()], CellValue::Blank, false),
        (vec![2.0.into(), (-2.0).into()], CellValue::Blank, false),
        // Pinned numeric admission still ignores selected cell errors; Calc differs.
        (
            vec![CellValue::Error(CellError::NA)],
            CellValue::Blank,
            true,
        ),
    ] {
        let mut data = vec!["N".into()];
        data.extend(values);
        let criteria = ["N".into(), condition];
        for function in [
            DatabaseFunction::Average,
            DatabaseFunction::Count,
            DatabaseFunction::Sum,
            DatabaseFunction::Min,
            DatabaseFunction::Max,
        ] {
            let plan = DatabasePlan::new(
                function,
                Grid::new(data.len(), 1, &data).unwrap(),
                &1.0.into(),
                Grid::new(2, 1, &criteria).unwrap(),
            )
            .unwrap();
            let pinned = plan.execute().unwrap();
            assert_eq!(
                plan.execute_with_policy(FinalizationPolicy::PinnedPoi)
                    .unwrap(),
                pinned
            );
            let modern = plan
                .execute_with_policy(FinalizationPolicy::ExcelDocumentation)
                .unwrap();
            if function == DatabaseFunction::Average {
                assert_eq!(pinned, 0.0.into());
                assert_eq!(
                    modern,
                    if empty {
                        CellValue::Error(CellError::Div0)
                    } else {
                        0.0.into()
                    }
                );
            } else {
                assert_eq!(modern, pinned);
            }
        }
    }
}
