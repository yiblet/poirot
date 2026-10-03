//! Source invariants from pinned POI and authenticated Commons Math 3.6.1.
use poirot_functions_core::CellError;
use poirot_functions_statistics::{normal_dist, normal_inverse, poisson};

#[test]
fn normal_center_and_source_tail_saturation() {
    assert_eq!(normal_dist(7.0, 7.0, 3.0, true), Ok(0.5));
    assert_eq!(normal_dist(-121.0, 0.0, 3.0, true), Ok(0.0));
    assert_eq!(normal_dist(121.0, 0.0, 3.0, true), Ok(1.0));
    assert_eq!(normal_inverse(0.5, 7.0, 3.0), Ok(7.0));
    for p in [0.0, 1.0, -0.1, 1.1] {
        assert_eq!(normal_inverse(p, 0.0, 1.0), Err(CellError::Num));
    }
    assert_eq!(normal_inverse(0.5, 0.0, 0.0), Err(CellError::Num));
}

#[test]
fn poisson_java_integer_boundary_and_truncation() {
    let edge = i32::MAX as f64;
    assert_eq!(poisson(edge, 3.0, true), Ok(1.0));
    assert_eq!(poisson(edge + 1.0, 3.0, true), Ok(1.0));
    assert_eq!(poisson(edge, 3.0, false), Ok(0.0));
    assert_eq!(poisson(2.9, 3.0, true), poisson(2.0, 3.0, true));
    assert_eq!(poisson(2.9, 3.0, false), poisson(2.0, 3.0, false));
    assert_eq!(poisson(0.0, 0.0, true), Ok(1.0));
    assert_eq!(poisson(0.0, 0.0, false), Ok(1.0));
}

#[test]
fn retained_inverse_case_exact_double() {
    let result = normal_inverse(0.75, 0.0, 1.0).unwrap();
    assert_eq!(result.to_bits(), 0.6744897501960816_f64.to_bits());
}

#[test]
fn retained_cumulative_cases_exact_doubles() {
    assert_eq!(
        normal_dist(1.0, 0.0, 1.0, true).unwrap().to_bits(),
        0.841344746068543_f64.to_bits()
    );
    assert_eq!(
        poisson(2.0, 3.0, true).unwrap().to_bits(),
        0.4231900811268693_f64.to_bits()
    );
}
