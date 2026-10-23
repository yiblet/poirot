//! Independently derived identities and multinomial integer counts.
use poirot_functions_core::CellError;
use poirot_functions_math::modern::*;
#[test]
fn reciprocal_extremes_and_inverse_domains() {
    assert_eq!(acot(0.).unwrap(), std::f64::consts::FRAC_PI_2);
    assert!(acot(1e308).unwrap() > 0.);
    assert_eq!(acoth(1.), Err(CellError::Num));
    assert_eq!(acoth(f64::INFINITY), Err(CellError::Num));
    assert_eq!(reciprocal(1000., Reciprocal::Sech), Ok(0.));
    assert_eq!(reciprocal(134217728., Reciprocal::Cot), Err(CellError::Num));
    assert_eq!(reciprocal(0., Reciprocal::Csch), Err(CellError::Div0));
    assert!(reciprocal(1e-10, Reciprocal::Csch).unwrap() > 9.99e9);
}
#[test]
fn multinomial_charges_bounded_steps_and_keeps_domain_errors() {
    let mut m = Multinomial::new();
    let mut work = 0;
    for n in [2., 3., 4.] {
        m.push(n, || {
            work += 1;
            Ok::<_, CellError>(())
        })
        .unwrap();
    }
    assert_eq!(m.finish(), 1260.);
    assert!(work > 0);
    let mut m = Multinomial::new();
    m.push(3., || Ok::<_, CellError>(())).unwrap();
    assert_eq!(m.push(4., || Err(CellError::Ref)), Err(CellError::Ref));
    assert_eq!(m.push(-1., || Ok::<_, CellError>(())), Err(CellError::Num));
}
