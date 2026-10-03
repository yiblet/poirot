//! Independent small tables derived from DStarRunner and algorithm source branches.
use poirot_functions_core::{CellError as E, CellValue as V};
use poirot_functions_database::{DatabaseError, DatabaseFunction as F, DatabasePlan, Grid};

fn run(
    f: F,
    data: &[V],
    columns: usize,
    field: V,
    criteria: &[V],
    width: usize,
) -> Result<V, DatabaseError> {
    DatabasePlan::new(
        f,
        Grid::new(data.len() / columns, columns, data).unwrap(),
        &field,
        Grid::new(criteria.len() / width, width, criteria).unwrap(),
    )?
    .execute()
}

#[test]
fn rounded_fields_and_floored_criteria_headings_have_distinct_boundaries() {
    let data = ["N".into(), "M".into(), 7.0.into(), 11.0.into()];
    let all = ["N".into(), V::Blank];
    for (field, expected) in [(1.4999999999999998, 7.), (1.5, 11.)] {
        assert_eq!(
            run(F::Sum, &data, 2, field.into(), &all, 1),
            Ok(expected.into())
        );
    }
    assert_eq!(
        run(F::Sum, &data, 2, 0.49999999999999994.into(), &all, 1),
        Err(DatabaseError::Cell(E::Value))
    );
    assert_eq!(
        run(F::Count, &data, 2, 0.49999999999999994.into(), &all, 1),
        Ok(1.0.into())
    );
    assert_eq!(
        run(F::Sum, &data, 2, 2.0.into(), &[1.9.into(), 7.0.into()], 1),
        Ok(11.0.into())
    );
    for heading in [2147483648., -2147483649., f64::NAN, f64::INFINITY] {
        assert_eq!(
            run(
                F::Sum,
                &data,
                2,
                1.0.into(),
                &[heading.into(), 7.0.into()],
                1
            ),
            Err(DatabaseError::Cell(E::Value))
        );
        // Blank conditions bypass even invalid heading coercion.
        assert_eq!(
            run(F::Sum, &data, 2, 1.0.into(), &[heading.into(), V::Blank], 1),
            Ok(7.0.into())
        );
    }
}

#[test]
fn duplicate_and_nontext_headers_select_the_first_matching_column() {
    let data = [
        "n".into(),
        "N".into(),
        42.0.into(),
        true.into(),
        V::Blank,
        3.0.into(),
        99.0.into(),
        5.0.into(),
        7.0.into(),
        11.0.into(),
    ];
    let all = [V::Blank, V::Blank];
    for (field, expected) in [("N", 3.), ("42", 5.), ("TRUE", 7.)] {
        assert_eq!(
            run(F::Sum, &data, 5, field.into(), &all, 1),
            Ok(expected.into())
        );
    }
    assert_eq!(
        run(F::Sum, &data, 5, "".into(), &all, 1),
        Err(DatabaseError::Cell(E::Value))
    );
    // TRUE is a numeric criteria heading selecting column one.
    assert_eq!(
        run(F::Sum, &data, 5, "42".into(), &[true.into(), 3.0.into()], 1),
        Ok(5.0.into())
    );
}

#[test]
fn numeric_conditions_coerce_text_but_numeric_operator_strings_do_not() {
    let data = [
        "N".into(),
        12.0.into(),
        "12".into(),
        " 0x1.8p3D ".into(),
        true.into(),
        V::Blank,
    ];
    assert_eq!(
        run(
            F::CountA,
            &data,
            1,
            "N".into(),
            &["N".into(), 12.0.into()],
            1
        ),
        Ok(3.0.into())
    );
    assert_eq!(
        run(
            F::CountA,
            &data,
            1,
            "N".into(),
            &["N".into(), "=12".into()],
            1
        ),
        Ok(1.0.into())
    );
    assert_eq!(
        run(
            F::CountA,
            &data,
            1,
            "N".into(),
            &["N".into(), true.into()],
            1
        ),
        Ok(1.0.into())
    );
    // Blank criteria is universal, empty-text criteria selects only StringEval.
    assert_eq!(
        run(F::Count, &data, 1, "N".into(), &["N".into(), "".into()], 1),
        Ok(0.0.into())
    );
}

#[test]
fn error_criteria_compare_codes_and_get_preserves_single_error() {
    let data = ["N".into(), V::Error(E::NA), V::Error(E::Div0), V::Blank];
    assert_eq!(
        run(
            F::CountA,
            &data,
            1,
            "N".into(),
            &["N".into(), V::Error(E::NA)],
            1
        ),
        Ok(1.0.into())
    );
    assert_eq!(
        run(
            F::Get,
            &data,
            1,
            "N".into(),
            &["N".into(), V::Error(E::NA)],
            1
        ),
        Ok(V::Error(E::NA))
    );
    assert_eq!(
        run(
            F::Sum,
            &data,
            1,
            "N".into(),
            &["N".into(), V::Error(E::NA)],
            1
        ),
        Ok(0.0.into())
    );
    assert_eq!(
        run(F::Get, &data, 1, "N".into(), &["N".into(), V::Blank], 1),
        Ok(V::Error(E::Num))
    );
}

#[test]
fn statistics_collect_only_numeric_evals_and_population_singleton_is_zero() {
    let data = [
        "N".into(),
        2.0.into(),
        true.into(),
        false.into(),
        "100".into(),
        V::Blank,
        V::Error(E::NA),
    ];
    let all = ["N".into(), V::Blank];
    for (f, expected) in [
        (F::Var, 1.),
        (F::VarP, 2. / 3.),
        (F::Stdev, 1.),
        (F::StdevP, (2f64 / 3.).sqrt()),
    ] {
        assert_eq!(run(f, &data, 1, 1.0.into(), &all, 1), Ok(expected.into()));
    }
    let singleton = ["N".into(), f64::MAX.into(), "100".into(), V::Blank];
    for f in [F::VarP, F::StdevP] {
        assert_eq!(run(f, &singleton, 1, 1.0.into(), &all, 1), Ok(0.0.into()));
    }
    for f in [F::Var, F::Stdev] {
        assert_eq!(
            run(f, &singleton, 1, 1.0.into(), &all, 1),
            Ok(V::Error(E::Div0))
        );
    }
}

#[test]
fn get_stops_before_later_unsupported_criteria_and_false_and_skips_bad_headers() {
    let data = ["N".into(), 1.0.into(), 1.0.into(), 9.0.into()];
    let criteria = [
        "N".into(),
        "Unknown".into(),
        1.0.into(),
        V::Blank,
        9.0.into(),
        true.into(),
    ];
    assert_eq!(
        run(F::Get, &data, 1, "N".into(), &criteria, 2),
        Ok(V::Error(E::Num))
    );
    assert_eq!(
        run(F::Sum, &data, 1, "N".into(), &criteria, 2),
        Err(DatabaseError::FormulaCriteria)
    );
    let criteria = ["N".into(), 2147483648.0.into(), 99.0.into(), 1.0.into()];
    assert_eq!(
        run(F::Sum, &data, 1, "N".into(), &criteria, 2),
        Ok(0.0.into())
    );
}
