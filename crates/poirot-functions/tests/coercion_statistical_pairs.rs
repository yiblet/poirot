use poirot_functions::coercion::CellError;
use poirot_functions::coercion::statistics::{
    NumericPairs, PairCollection, Value, coercev2_statistics_pairs,
};

#[test]
fn numeric_samples_exclude_both_coordinates_of_every_nonnumeric_pair() {
    // A deliberately extreme opposite coordinate exposes independent filtering or
    // accidental parsing. Zero is an actual observation, not a missing value.
    for excluded in [
        Value::Blank,
        Value::Boolean(true),
        Value::Boolean(false),
        Value::Text {
            numeric: Some(1000.),
        },
        Value::Text { numeric: None },
    ] {
        for left in [true, false] {
            let (a, b) = if left {
                (excluded, Value::Number(999.))
            } else {
                (Value::Number(999.), excluded)
            };
            let pairs = coercev2_statistics_pairs(
                PairCollection::NumericOnly,
                &[Value::Number(0.), a, Value::Number(2.)],
                &[Value::Number(1.), b, Value::Number(5.)],
            )
            .unwrap();
            assert_eq!(pairs.x, [0., 2.]);
            assert_eq!(pairs.y, [1., 5.]);
            assert_eq!(pairs.original_len, 3);
            assert_eq!(pairs.sample_len(), Ok(2));
        }
    }
}

#[test]
fn input_shape_and_error_precedence_are_checked_before_sample_membership() {
    assert_eq!(
        coercev2_statistics_pairs(
            PairCollection::NumericOnly,
            &[Value::Text { numeric: None }],
            &[]
        ),
        Err(CellError::NA)
    );
    assert_eq!(
        coercev2_statistics_pairs(
            PairCollection::NumericOnly,
            &[
                Value::Text { numeric: Some(1.) },
                Value::Error(CellError::Ref)
            ],
            &[Value::Error(CellError::Div0), Value::Blank]
        ),
        Err(CellError::Ref)
    );
    let pairs = coercev2_statistics_pairs(
        PairCollection::NumericOnly,
        &[Value::Boolean(true)],
        &[Value::Number(1.)],
    )
    .unwrap();
    assert_eq!(pairs.sample_len(), Ok(0));
}

#[test]
fn sample_length_validates_alignment_and_retains_original_shape() {
    let mut pairs = NumericPairs {
        x: vec![1., 2.],
        y: vec![3.],
        original_len: 3,
    };
    assert_eq!(pairs.sample_len(), Err(CellError::NA));
    pairs.y.push(4.);
    assert_eq!(pairs.sample_len(), Ok(2));
    pairs.original_len = 1;
    assert_eq!(pairs.sample_len(), Err(CellError::NA));
}
