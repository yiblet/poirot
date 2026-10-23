//! Expected results follow exact small least-squares systems, not spreadsheet outputs.
use super::*;
use crate::{TrendFitPolicy, trend_with_policy};
const B: Budget = Budget {
    max_cells: 100000,
    max_work: 1000000,
};
fn m(n: usize, p: usize, x: &[f64]) -> Matrix<f64> {
    Matrix::new(n, p, x.to_vec()).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10 * b.abs().max(1.0), "{a} != {b}");
}
#[test]
fn rational_fit_checks_every_diagnostic() {
    let fit = least_squares(
        &m(3, 1, &[0., 1., 2.]),
        &[1., 2., 2.],
        Intercept::Estimate,
        B,
    )
    .unwrap();
    near(fit.coefficients().unwrap()[0].unwrap(), 0.5);
    near(fit.intercept().unwrap(), 7. / 6.);
    let d = fit.diagnostics(B).unwrap();
    near(d.standard_errors[0].unwrap(), (1f64 / 12.).sqrt());
    near(d.intercept_error.unwrap(), 5f64.sqrt() / 6.);
    near(d.r_squared.unwrap(), 0.75);
    near(d.residual_error.unwrap(), (1f64 / 6.).sqrt());
    near(d.f.unwrap(), 3.);
    assert_eq!(d.degrees, 1);
    near(d.regression_squares.unwrap(), 0.5);
    near(d.residual_squares.unwrap(), 1. / 6.);
}
#[test]
fn constrained_fit_uses_uncentered_total() {
    let fit = least_squares(&m(2, 1, &[1., 2.]), &[1., 1.], Intercept::Zero, B).unwrap();
    near(fit.coefficients().unwrap()[0].unwrap(), 0.6);
    assert_eq!(fit.intercept(), Ok(0.));
    let d = fit.diagnostics(B).unwrap();
    near(d.r_squared.unwrap(), 0.9);
    near(d.residual_squares.unwrap(), 0.2);
    assert_eq!(d.degrees, 1);
    assert_eq!(d.intercept_error, Err(CellError::NA));
}
#[test]
fn redundant_predictors_are_removed_not_spread() {
    let x = m(4, 3, &[0., 0., 0., 1., 10., 0., 2., 20., 0., 3., 30., 0.]);
    let fit = least_squares(&x, &[1., 3., 5., 7.], Intercept::Estimate, B).unwrap();
    assert_eq!(fit.rank(), 1);
    assert_eq!(fit.active_predictors().unwrap(), vec![true, false, false]);
    let b = fit.coefficients().unwrap();
    near(b[0].unwrap(), 2.);
    assert_eq!(b[1], Ok(0.));
    assert_eq!(b[2], Ok(0.));
    let d = fit.diagnostics(B).unwrap();
    assert_eq!(d.degrees, 2);
    assert_eq!(d.standard_errors[1], Ok(0.));
    near(
        fit.predict(&m(1, 3, &[4., 40., 0.]), B).unwrap().values()[0],
        9.,
    );
}
#[test]
fn prediction_preserves_large_offset_differences_and_column_scales() {
    let x = m(
        4,
        2,
        &[1e15, 0., 1e15 + 1., 0., 1e15, 1e-100, 1e15 + 1., 1e-100],
    );
    let fit = least_squares(&x, &[1., 3., 4., 6.], Intercept::Estimate, B).unwrap();
    let b = fit.coefficients().unwrap();
    near(b[0].unwrap(), 2.);
    near(b[1].unwrap() / 1e100, 3.);
    near(
        fit.predict(&m(1, 2, &[1e15 + 2., 3e-100]), B)
            .unwrap()
            .values()[0],
        14.,
    );
}
#[test]
fn rank_zero_and_determined_models_keep_coefficients_when_stats_unavailable() {
    let fit = least_squares(&m(1, 2, &[3., 7.]), &[5.], Intercept::Estimate, B).unwrap();
    assert_eq!(fit.rank(), 0);
    assert_eq!(fit.intercept(), Ok(5.));
    assert_eq!(fit.diagnostics(B).unwrap().degrees, 0);
    assert_eq!(fit.predict(&m(1, 2, &[9., 2.]), B).unwrap().values(), &[5.]);
    let fit = least_squares(&m(2, 2, &[1., 0., 0., 1.]), &[2., 3.], Intercept::Zero, B).unwrap();
    assert_eq!(fit.rank(), 2);
    assert_eq!(
        fit.diagnostics(B).unwrap().residual_error,
        Err(CellError::Div0)
    );
}
#[test]
fn rank_admission_and_bad_shapes_remain_fallible() {
    assert_eq!(
        regression_budget(usize::MAX, 2, 1, true),
        Err(Error::SizeOverflow)
    );
    assert!(matches!(
        least_squares(
            &m(2, 1, &[0., 1.]),
            &[1., 2.],
            Intercept::Estimate,
            Budget {
                max_cells: 1,
                max_work: 1
            }
        ),
        Err(Error::BudgetExceeded)
    ));
    assert!(matches!(
        least_squares(&m(2, 1, &[0., 1.]), &[1.], Intercept::Estimate, B),
        Err(Error::Formula(CellError::Ref))
    ));
}

#[test]
fn finite_coefficients_do_not_require_an_overflowing_projection_product() {
    let x = m(2, 2, &[1e308, 1e308, 5e307, 1e308]);
    let fit = least_squares(&x, &[1e308, -1e308], Intercept::Zero, B).unwrap();
    let b = fit.coefficients().unwrap();
    near(b[0].unwrap(), 4.);
    near(b[1].unwrap(), -3.);
}

#[test]
fn modern_pure_trend_uses_the_same_rank_and_axis_policy() {
    let result = trend_with_policy(
        &m(1, 4, &[1., 3., 4., 6.]),
        Some(&m(2, 4, &[0., 1., 0., 1., 0., 0., 1., 1.])),
        Some(&m(2, 2, &[2., 4., 3., 5.])),
        true,
        B,
        TrendFitPolicy::Determined,
    )
    .unwrap();
    assert_eq!((result.rows(), result.cols()), (1, 2));
    near(result.values()[0], 14.);
    near(result.values()[1], 24.);
    assert_eq!(
        trend_with_policy(
            &m(1, 1, &[3.]),
            Some(&m(1, 1, &[1.])),
            Some(&m(1, 1, &[3.])),
            true,
            B,
            TrendFitPolicy::Determined
        ),
        Err(Error::Formula(CellError::Value))
    );
    let fit = least_squares(&m(1, 1, &[1.]), &[3.], Intercept::Estimate, B).unwrap();
    assert_eq!(fit.predict(&m(1, 1, &[3.]), B).unwrap().values(), &[3.]);
    assert_eq!(
        fit.predict_trend(&m(1, 1, &[3.]), B),
        Err(Error::Formula(CellError::Value))
    );
}

#[test]
fn exact_zero_slopes_do_not_normalize_irrelevant_prediction_coordinates() {
    let fit = least_squares(&m(2, 1, &[0., 1e-300]), &[5., 5.], Intercept::Estimate, B).unwrap();
    assert_eq!(fit.predict(&m(1, 1, &[1e308]), B).unwrap().values(), &[5.]);
    let fit = least_squares(
        &m(3, 2, &[0., 0., 1., 1e-300, 2., 2e-300]),
        &[1., 3., 5.],
        Intercept::Estimate,
        B,
    )
    .unwrap();
    near(
        fit.predict(&m(1, 2, &[3., 1e308]), B).unwrap().values()[0],
        7.,
    );
}

#[test]
fn finite_extrapolation_recovers_original_units_after_normalization_overflow() {
    let fit = least_squares(
        &m(2, 1, &[0., 1e-300]),
        &[0., 1e-300],
        Intercept::Estimate,
        B,
    )
    .unwrap();
    near(
        fit.predict(&m(1, 1, &[1e308]), B).unwrap().values()[0] / 1e308,
        1.,
    );
}
