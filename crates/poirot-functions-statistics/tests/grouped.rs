//! Independent reduction and admission identities for contextual aggregation handles.
use poirot_functions_statistics::{BuiltinAggregator as A, Collection, median};
#[test]
fn reductions_preserve_empty_and_extrema_contracts() {
    for (operation, values, expected) in [
        (A::Average, vec![2.0, 8.0, 20.0], 10.0),
        (A::Minimum, vec![3.0, 8.0], 3.0),
        (A::Maximum, vec![-8.0, -3.0], -3.0),
        (A::Median, vec![8.0, 2.0, 5.0], 5.0),
        (A::Product, vec![2.0, 3.0, 4.0], 24.0),
        (A::VariancePopulation, vec![2.0, 4.0], 1.0),
        (A::DevSq, vec![2.0, 4.0], 2.0),
    ] {
        assert_eq!(operation.numeric(&mut values.clone()).unwrap(), expected);
    }
    assert!(A::Average.numeric(&mut []).is_err());
    assert_eq!(A::Product.numeric(&mut []).unwrap(), 0.0);
    assert_eq!(A::Maximum.numeric(&mut []).unwrap(), 0.0);
    let mut values = [f64::MAX, f64::MAX];
    assert_eq!(median(&mut values).unwrap(), f64::MAX);
    assert_eq!(A::AverageA.collection(), Collection::AggregateA);
    assert_eq!(A::MinimumA.collection(), Collection::ExtremaA);
}
