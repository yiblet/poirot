//! Independent controls from translation invariance, elementary sums and tail integrals.
use super::*;
fn near(actual: Result<()>, expected: f64) {
    let actual = actual.unwrap();
    assert!(
        (actual - expected).abs() <= 2e-12 * expected.abs().max(1.0),
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn centered_moments_preserve_translation_and_scale() {
    for scale in [1.0, 1e200, 1e-200] {
        near(moment(&[-scale, 0., scale], MomentKind::SkewSample), 0.);
        near(
            moment(&[-scale, -scale, scale, scale], MomentKind::Kurtosis),
            -6.,
        );
    }
    for x in [2f64.powi(40), 2f64.powi(500), -2f64.powi(500)] {
        near(
            moment(&[x, x, x.next_up()], MomentKind::SkewSample),
            3f64.sqrt(),
        );
        near(moment(&[x, x, x, x.next_up()], MomentKind::Kurtosis), 4.);
        near(
            moment(&[x, x, x.next_up()], MomentKind::SkewPopulation),
            1. / 2f64.sqrt(),
        );
    }
    assert_eq!(
        moment::<()>(&[1., 1., 1.], MomentKind::SkewPopulation),
        Err(error(CellError::Div0))
    );
    assert_eq!(
        moment::<()>(&[1., 2.], MomentKind::SkewPopulation),
        Err(error(CellError::Div0))
    );
}
#[test]
fn harmonic_modes_quantiles_ranks_and_trim() {
    near(harmonic_mean(&[2., 6.]), 3.);
    near(harmonic_mean(&[1e-300, 3e-300]), 1.5e-300);
    assert_eq!(harmonic_mean::<()>(&[0., 1.]), Err(error(CellError::Num)));
    let mut modes_data = [7., 2., 7., 2., -0., 0.];
    assert_eq!(modes(&mut modes_data, ()), Ok(3));
    assert_eq!(&modes_data[..3], &[7., 2., 0.]);
    assert_eq!(modes(&mut [1., 2.], ()), Err(error(CellError::NA)));
    near(percentile(&mut [0., 10., 20.], 0.375), 5.);
    near(percentile(&mut [-f64::MAX, f64::MAX], 0.5), 0.);
    assert_eq!(
        percentile::<()>(&mut [0., 10., 20.], 0.2),
        Err(error(CellError::Num))
    );
    near(rank_average(&[2., 2., 7.], 2., false), 2.5);
    assert_eq!(
        rank_average::<()>(&[1., 2.], 3., false),
        Err(error(CellError::NA))
    );
    near(trimmed_mean(&mut [100., 1., 2., 3., -100.], 0.4), 2.);
    assert_eq!(
        trimmed_mean::<()>(&mut [1., 2.], 1.),
        Err(error(CellError::Div0))
    );
}
#[test]
fn scalar_transforms_and_permutations() {
    near(fisher(0.5, false), 3f64.ln() * 0.5);
    near(fisher(1e300, true), 1.);
    assert_eq!(fisher::<()>(1., false), Err(error(CellError::Num)));
    near(permutations(5.9, 3.9, false), 60.);
    near(permutations(2., 4., true), 16.);
    near(permutations(0., 0., true), 1.);
    assert_eq!(
        permutations::<()>(1e300, 1e299, false),
        Err(error(CellError::Num))
    );
}
#[test]
fn independently_scaled_regression_uses_residuals() {
    near(paired(&[1., 0., 1.], &[0., 1., 2.], PairKind::RSquared), 0.);
    near(
        paired(&[1., 0., 1.], &[0., 1., 2.], PairKind::StandardError),
        (2f64 / 3.).sqrt(),
    );
    near(
        paired(
            &[1e200, 2e200, 3e200],
            &[1e-200, 2e-200, 3e-200],
            PairKind::RSquared,
        ),
        1.,
    );
    near(
        paired(
            &[1e200, 2e200, 3e200],
            &[1e-200, 2e-200, 3e-200],
            PairKind::StandardError,
        ),
        0.,
    );
    assert_eq!(
        paired::<()>(&[1., 2., 3.], &[1., 1., 1.], PairKind::StandardError),
        Err(error(CellError::Div0))
    );
}
#[test]
fn independent_hypothesis_probabilities_and_work_failure() {
    near(f_test(&[0., 1., 2.], &[0., 1., 2.], |_| Ok(())), 1.);
    for kind in [TestKind::Pooled, TestKind::Welch] {
        near(t_test(&[0., 2.], &[0., 1., 2.], kind, true, |_| Ok(())), 1.);
        near(
            t_test(&[0., 2.], &[0., 1., 2.], kind, false, |_| Ok(())),
            0.5,
        );
    }
    // Welch t²=3, nu=32/19; independent 100-digit beta defining integral.
    near(
        t_test(&[0., 2.], &[2., 3., 4.], TestKind::Welch, true, |_| Ok(())),
        0.2481885380840121,
    );
    near(
        chi_square_test(&[3., 1., 1., 3.], &[2.; 4], 2, 2, |_| Ok(())),
        libm::erfc(1.),
    );
    near(z_test(&[0., 1., 2.], 1., None), 0.5);
    near(z_test(&[0., 1., 2.], 1., Some(1.)), 0.5);
    near(probability(&[0., 1., 2.], &[0.25, 0.25, 0.5], 1., 2.), 0.75);
    assert_eq!(
        probability::<()>(&[0., 1.], &[0., 1.], 0., 1.),
        Err(error(CellError::Num))
    );
    assert_eq!(
        t_test(&[0., 2.], &[2., 3., 4.], TestKind::Welch, true, |_| Err(
            "stop"
        )),
        Err(DistributionError::Operational("stop"))
    );
}

#[test]
fn raw_subnormal_z_mean_difference_keeps_sign() {
    let u = f64::from_bits(1);
    near(z_test(&[3.0 * u], 4.0 * u, Some(u)), 0.8413447460685429);
}

#[test]
fn chi_square_positive_overflow_saturates_before_compensation() {
    near(
        chi_square_test(&[f64::MAX, f64::MAX], &[1., 1.], 1, 2, |_| Ok(())),
        0.,
    );
    near(
        chi_square_test(&[1e154, 1e154], &[1., 1.], 1, 2, |_| Ok(())),
        0.,
    );
    let value = harmonic_mean::<()>(&[1e-300, 3e-300]).unwrap();
    assert!((value / 1.5e-300 - 1.0).abs() < 1e-14);
}

#[test]
fn translated_group_means_and_nontrivial_pooled_and_paired_statistics() {
    near(
        t_test(&[0., 2.], &[2., 3., 4.], TestKind::Pooled, true, |_| Ok(())),
        0.154_030_903_000_936_2,
    );
    near(
        t_test(&[0., 2., 4.], &[1., 4., 7.], TestKind::Paired, true, |_| {
            Ok(())
        }),
        0.07417990022744854,
    );
    let x = 2f64.powi(500);
    let step = x.next_up() - x;
    near(
        t_test(
            &[x, x + 2. * step],
            &[x + 2. * step, x + 3. * step, x + 4. * step],
            TestKind::Welch,
            true,
            |_| Ok(()),
        ),
        0.2481885380840121,
    );
    near(z_test(&[x, x + step, x + 2. * step], x + step, None), 0.5);
    near(f_test(&[0., 1., 2.], &[0., 2., 4.], |_| Ok(())), 0.4);
}

#[test]
fn mean_comparison_resolves_wide_sample_cancellation_before_tiny_hypothesis() {
    near(
        z_test(&[-1e200, 1e200], 1e-200, Some(1e-200)),
        0.9213503964748574,
    );
    let u = f64::from_bits(1);
    near(z_test(&[0., u], 0., Some(u)), 0.23975006109347674);
    near(z_test(&[0., u], u, Some(u)), 0.7602499389065233);
}
