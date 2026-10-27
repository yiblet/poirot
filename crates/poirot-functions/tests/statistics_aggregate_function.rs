//! AGGREGATE expectations follow Microsoft's selector table and underlying definitions:
//! https://support.microsoft.com/en-us/excel/functions/aggregate-function
//! https://support.microsoft.com/en-us/excel/functions/quartile-inc-function
//! https://support.microsoft.com/en-us/excel/functions/percentile-exc-function
//! No upstream implementation or fixtures are copied; small samples below have
//! hand-computed means, denominators, ranks and linear interpolation.
use poirot_functions::scalar::CellError;
use poirot_functions::statistics::{AggregateFunction as A, AggregateInput};

fn calculate(selector: u8, values: &[f64], k: Option<f64>) -> Result<f64, CellError> {
    A::from_number(selector)?.calculate(
        AggregateInput {
            numbers: values,
            nonempty_count: values.len(),
        },
        k,
    )
}

#[test]
fn all_nineteen_selectors_match_independent_arithmetic() {
    // Mean=3, squared deviations=10, sample denominator=4, population=5.
    let values = [1.0, 2.0, 3.0, 4.0, 5.0];
    for (selector, k, expected) in [
        (1, None, 3.0),
        (2, None, 5.0),
        (3, None, 5.0),
        (4, None, 5.0),
        (5, None, 1.0),
        (6, None, 120.0),
        (7, None, 2.5_f64.sqrt()),
        (8, None, 2.0_f64.sqrt()),
        (9, None, 15.0),
        (10, None, 2.5),
        (11, None, 2.0),
        (12, None, 3.0),
        (14, Some(2.0), 4.0),
        (15, Some(2.0), 2.0),
        (16, Some(0.25), 2.0),
        (17, Some(1.0), 2.0),
        (18, Some(0.25), 1.5),
        (19, Some(1.0), 1.5),
    ] {
        let actual = calculate(selector, &values, k).unwrap();
        assert!(
            (actual - expected).abs() < 1e-12,
            "selector {selector}: {actual}"
        );
    }
    assert_eq!(calculate(13, &[3., 2., 3., 2.], None), Ok(2.0));
}

#[test]
fn counts_distinguish_numeric_from_nonblank_members() {
    // Host admitted two numbers, text (including empty text), a logical and an error.
    let input = AggregateInput {
        numbers: &[2., 4.],
        nonempty_count: 6,
    };
    assert_eq!(A::Count.calculate(input, None), Ok(2.));
    assert_eq!(A::CountA.calculate(input, None), Ok(6.));
    assert_eq!(A::Average.calculate(input, None), Ok(3.));
    let invalid = AggregateInput {
        numbers: &[1.],
        nonempty_count: 0,
    };
    assert_eq!(A::CountA.calculate(invalid, None), Err(CellError::Value));
}

#[test]
fn empty_samples_and_population_denominators_are_explicit() {
    for selector in [2, 3, 4, 5, 6, 9] {
        assert_eq!(calculate(selector, &[], None), Ok(0.));
    }
    for selector in [1, 7, 8, 10, 11] {
        assert_eq!(calculate(selector, &[], None), Err(CellError::Div0));
    }
    assert_eq!(calculate(12, &[], None), Err(CellError::Num));
    assert_eq!(calculate(13, &[], None), Err(CellError::NA));
    for selector in [7, 10] {
        assert_eq!(calculate(selector, &[8.], None), Err(CellError::Div0));
    }
    for selector in [8, 11] {
        assert_eq!(calculate(selector, &[8.], None), Ok(0.));
    }
    assert_eq!(calculate(13, &[1., 2.], None), Err(CellError::NA));
}

#[test]
fn parameters_and_percentile_endpoints_are_validated() {
    for selector in 14..=19 {
        assert_eq!(
            calculate(selector, &[1., 2., 3.], None),
            Err(CellError::Value)
        );
        assert_eq!(
            calculate(selector, &[1., 2., 3.], Some(f64::NAN)),
            Err(CellError::Num)
        );
    }
    for selector in [0, 20, 255] {
        assert_eq!(A::from_number(selector), Err(CellError::Value));
    }
    for k in [0., -1., 4.] {
        assert_eq!(calculate(14, &[1., 2., 3.], Some(k)), Err(CellError::Num));
        assert_eq!(calculate(15, &[1., 2., 3.], Some(k)), Err(CellError::Num));
    }
    for (k, expected) in [(0., 1.), (1., 3.)] {
        assert_eq!(calculate(16, &[1., 2., 3.], Some(k)), Ok(expected));
        assert_eq!(calculate(18, &[1., 2., 3.], Some(k)), Err(CellError::Num));
    }
    assert_eq!(calculate(17, &[1., 2., 3.], Some(1.9)), Ok(1.5));
    assert_eq!(calculate(19, &[1., 2., 3.], Some(1.9)), Ok(1.));
    assert_eq!(calculate(18, &[1., 2., 3.], Some(0.1)), Err(CellError::Num));
    assert_eq!(calculate(9, &[1., 2.], Some(f64::NAN)), Ok(3.));
}

#[test]
fn large_samples_extremes_and_nonfinite_values() {
    let values: Vec<_> = (0..10_000).map(f64::from).collect();
    assert_eq!(calculate(16, &values, Some(0.5)), Ok(4999.5));
    assert_eq!(calculate(16, &[-f64::MAX, f64::MAX], Some(0.5)), Ok(0.));
    assert_eq!(calculate(12, &[f64::MAX, f64::MAX], None), Ok(f64::MAX));
    for selector in [1, 4, 5, 6, 9, 12, 13, 16] {
        assert_eq!(
            calculate(selector, &[f64::INFINITY], Some(0.5)),
            Err(CellError::Num)
        );
    }
    assert_eq!(calculate(6, &[f64::MAX, 2.], None), Err(CellError::Num));
}
