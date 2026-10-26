//! Independent elementary identities and high-precision mathematical controls.
use super::*;
fn meter(_: u64) -> std::result::Result<(), ()> {
    Ok(())
}
fn near(actual: Result<()>, expected: f64, relative: f64) {
    let actual = actual.unwrap();
    assert!(
        (actual - expected).abs() <= relative * expected.abs().max(f64::MIN_POSITIVE),
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn elementary_continuous_controls() {
    near(beta_dist(4., 1., 1., false, 2., 6., meter), 0.25, 1e-14);
    near(beta_dist(4., 1., 1., true, 2., 6., meter), 0.5, 1e-14);
    near(beta_dist(0.5, 2., 2., false, 0., 1., meter), 1.5, 1e-14);
    near(gamma_value(-0.5), -2. * std::f64::consts::PI.sqrt(), 1e-14);
    near(gamma_log(5.), 24f64.ln(), 1e-14);
    near(phi(0.), 1. / std::f64::consts::TAU.sqrt(), 1e-14);
    near(gauss(1e-20), 1e-20 / std::f64::consts::TAU.sqrt(), 1e-14);
    for v in [
        exponential(2., 0.5, true),
        weibull(2., 1., 2., true),
        gamma_dist(2., 1., 2., true, meter),
        chi_square(2., 2., DistributionKind::Lower, meter),
    ] {
        near(v, 1. - (-1f64).exp(), 1e-14);
    }
    near(
        fisher_f(1e20, 2., 2., DistributionKind::Upper, meter),
        1e-20,
        1e-14,
    );
    near(
        fisher_f(1e308, 2., 2., DistributionKind::Upper, meter),
        1e-308,
        1e-13,
    );
    near(
        weibull(1e200, 2., 1e200, false),
        2. * (-1f64).exp() * 1e-200,
        1e-13,
    );
}
#[test]
fn discrete_counting_controls() {
    near(binomial(2., 4., 0.5, false, meter), 6. / 16., 1e-14);
    near(binomial(2., 4., 0.5, true, meter), 11. / 16., 1e-14);
    near(binomial_range(4., 0.5, 1., 3., meter), 14. / 16., 1e-14);
    near(negative_binomial(2., 1., 0.5, false, meter), 1. / 8., 1e-14);
    near(negative_binomial(2., 1., 0.5, true, meter), 7. / 8., 1e-14);
    near(
        hypergeometric(1., 2., 2., 6., false, meter),
        8. / 15.,
        1e-14,
    );
    near(
        hypergeometric(1., 2., 2., 6., true, meter),
        14. / 15.,
        1e-14,
    );
}
#[test]
fn incomplete_tails_retain_endpoint_and_subnormal_information() {
    let x = 2f64.powi(-50);
    let expected = 2. / std::f64::consts::PI * x.sqrt().asin();
    near(
        tails::beta(0.5, 0.5, BetaCoordinate::Lower(x), Tail::Lower, meter),
        expected,
        1e-13,
    );
    near(
        tails::beta(0.5, 0.5, BetaCoordinate::Upper(x), Tail::Upper, meter),
        expected,
        1e-13,
    );
    near(
        tails::beta(1e10, 1e10, BetaCoordinate::Lower(0.5), Tail::Lower, meter),
        0.5,
        1e-14,
    );
    near(tails::gamma(1., 1e-20, Tail::Lower, meter), 1e-20, 1e-14);
    let actual = tails::gamma(1., 740., Tail::Upper, meter).unwrap();
    // Independently rounded exp(-740), generated at 90 decimal digits.
    assert_eq!(actual.to_bits().abs_diff(4.2e-322f64.to_bits()), 0);
}
#[test]
fn gamma_large_shape_controls() {
    // mpmath's independent high-precision incomplete-gamma continued fraction.
    for (a, x, q) in [
        (128., 128., 0.4882455490958771),
        (128., 128. + 3. * 128f64.sqrt(), 0.0025677228315927864),
        (10000., 10000., 0.4986701916600448),
        (5e9, 5e9, 0.499_998_119_368_054_8),
        (5e9, 5e9 + 3. * 5e9f64.sqrt(), 0.0013500651719752106),
        (5e9, 5e9 - 3. * 5e9f64.sqrt(), 0.998650269098965),
    ] {
        near(tails::gamma(a, x, Tail::Upper, meter), q, 3e-11);
    }
}
#[test]
fn work_and_numerical_limits_remain_distinct() {
    assert_eq!(
        tails::beta(2., 3., BetaCoordinate::Lower(0.25), Tail::Lower, |_| Err(
            "stop"
        )),
        Err(Failure::Operational("stop"))
    );
    let mut calls = 0;
    let failure = tails::gamma(128., 128., Tail::Lower, |_| {
        calls += 1;
        if calls == 7 { Err("later") } else { Ok(()) }
    });
    assert_eq!(failure, Err(Failure::Operational("later")));
    assert_eq!(
        tails::gamma(1e13, 1e13, Tail::Lower, meter),
        Err(Failure::ComputationalLimit)
    );
    assert_eq!(
        binomial(5., 4., 0.5, false, meter),
        Err(Failure::Spreadsheet(CellError::Num))
    );
}
#[test]
fn transition_work_stays_bounded_across_shapes() {
    for a in [128., 10000., 1e6, 5e9] {
        for z in [-3., 0., 3., 30.] {
            let mut calls = 0;
            let result = tails::gamma(
                a,
                a + z * a.sqrt(),
                if z < 0. { Tail::Lower } else { Tail::Upper },
                |n| {
                    calls += n;
                    Ok::<_, ()>(())
                },
            );
            assert!(result.is_ok());
            assert!(calls < 1500);
        }
    }
    for (a, b, x) in [(2e9, 1e9, 2. / 3.), (1e10, 1e10, 0.500001), (2., 5., 0.2)] {
        let mut calls = 0;
        let result = tails::beta(a, b, BetaCoordinate::Lower(x), Tail::Lower, |n| {
            calls += n;
            Ok::<_, ()>(())
        });
        assert!(result.is_ok());
        assert!(calls < 1500);
    }
}
#[test]
fn large_shape_beta_reference_controls() {
    // Independent 80/120-digit tanh–sinh integrals at the exact supplied binary64 coordinates.
    near(
        tails::beta(2e9, 1e9, BetaCoordinate::Lower(2. / 3.), Tail::Lower, meter),
        0.4999982832240533,
        2e-13,
    );
    near(
        tails::beta(
            1e10,
            1e10,
            BetaCoordinate::Lower(0.500001),
            Tail::Lower,
            meter,
        ),
        0.61135129460711,
        2e-13,
    );
}
#[test]
fn gamma_tail_reference_grid_includes_positive_subnormals() {
    for (a, x, tail, expected) in [
        (
            128.,
            14.862915010152392,
            Tail::Lower,
            1.0999570844778418e-72,
        ),
        (5e9, 4997878679.656441, Tail::Lower, 4.3201286918085546e-198),
        (5e9, 5002121320.343559, Tail::Upper, 5.572494067370442e-198),
    ] {
        near(tails::gamma(a, x, tail, meter), expected, 3e-12);
    }
    let actual = tails::gamma(5e9, 5002687005.768509, Tail::Upper, meter).unwrap();
    assert!(actual > 0.0);
    assert!(actual.to_bits().abs_diff(3.7368308e-316f64.to_bits()) <= 2);
}
#[test]
fn scaled_inputs_and_small_shapes_preserve_representable_tails() {
    let minimum = f64::from_bits(1);
    let maximum = f64::MAX;
    // P(1/2,z) = erf(sqrt(z)) ~ 2*sqrt(z/pi), with negligible O(z^1.5).
    // Independently rounded at 100 digits. Relative tolerances cannot resolve subnormal ulps.
    let gamma = gamma_dist(minimum, 0.5, maximum, true, meter).unwrap();
    assert!(gamma.to_bits().abs_diff(1.8706374e-316f64.to_bits()) <= 2);
    let beta = beta_dist(minimum, 0.5, 1., true, 0., maximum, meter).unwrap();
    assert!(beta.to_bits().abs_diff(1.6578092e-316f64.to_bits()) <= 2);
    // Gamma shape approaching zero: Q(a,.5)/a tends to E1(.5).
    // Independent 100-digit exponential-integral value; next correction is O(a^2).
    near(
        tails::gamma(1e-200, 0.5, Tail::Upper, meter),
        5.597735947761608e-201,
        3e-13,
    );
    // Beta(1/2,1) lower tail sqrt(x), with a retained log endpoint below binary64.
    near(
        tails::beta(
            0.5,
            1.,
            BetaCoordinate::LowerLog(-1000.),
            Tail::Lower,
            meter,
        ),
        (-500f64).exp(),
        1e-13,
    );
}
#[test]
fn ordinary_coordinates_and_steep_scale_ratios_do_not_round_trip_through_logs() {
    near(
        beta_dist(2. / 3., 2e9, 1e9, true, 0., 1., meter),
        0.4999982832240533,
        2e-13,
    );
    near(
        weibull(1.5000000000000002, 1e16, 1.5, true),
        0.9876518890680446,
        2e-13,
    );
    near(weibull(3., 2., 1.5, true), 1. - (-4f64).exp(), 1e-14);
    near(
        fisher_f(3., 2., 2., DistributionKind::Lower, meter),
        0.75,
        1e-14,
    );
}
#[test]
fn tiny_beta_shapes_use_positive_tails_and_elementary_log_coordinates() {
    for a in [1e-10, 1e-100, 1e-300] {
        let log = a * 0.1f64.ln() + (a * 0.9).ln_1p();
        near(
            tails::beta(a, 2., BetaCoordinate::Lower(0.1), Tail::Upper, meter),
            -log.exp_m1(),
            3e-12,
        );
    }
    near(
        tails::beta(
            1e-300,
            1.,
            BetaCoordinate::LowerLog(-1000.),
            Tail::Upper,
            meter,
        ),
        1e-297,
        3e-13,
    );
    near(
        tails::beta(
            1e-200,
            1e-200,
            BetaCoordinate::Lower(0.1),
            Tail::Upper,
            meter,
        ),
        0.5,
        3e-13,
    );
    let x = f64::from_bits(1f64.to_bits() - 1);
    let expected = -((1e-300) * ((1.0 - x).ln() - 1e308f64.ln())).exp_m1();
    near(
        beta_dist(x, 1., 1e-300, true, -1e308, 1., meter),
        expected,
        3e-12,
    );
}
#[test]
fn positive_small_shape_beta_integrals_have_independent_controls() {
    for (a, b, x, q) in [
        (1e-10, 3.5, 0.1, 8.629429549712894e-11),
        (1e-200, 0.5, 0.1, 3.636_892_918_464_133e-200),
        (1e-300, 1e12, 1e-14, 4.037929576538604e-300),
    ] {
        near(
            tails::beta(a, b, BetaCoordinate::Lower(x), Tail::Upper, meter),
            q,
            3e-12,
        );
    }
    near(
        binomial(1., 1e12, 1e-20, false, meter),
        1e-8 * ((1e12 - 1.) * (-1e-20f64).ln_1p()).exp(),
        3e-13,
    );
}

#[test]
fn mirrored_small_beta_shapes_preserve_a_tiny_lower_tail() {
    near(
        tails::beta(0.5, 1e-200, BetaCoordinate::Lower(0.9), Tail::Lower, meter),
        3.636_892_918_464_133e-200,
        3e-12,
    );
}

#[test]
fn tiny_mirrored_beta_control_follows_the_elementary_integral() {
    near(
        beta_dist(0.9, 0.5, 1e-300, true, 0., 1., meter),
        2e-300 * 0.9f64.sqrt().atanh(),
        3e-12,
    );
}

#[test]
fn small_shape_endpoint_layers_use_a_proved_underflow_bound() {
    // For a<1, Q<=x^(a-1)*(1-x)^b/(b*B(a,b)); at these inputs log(bound)<-10^7.
    for x in [0.5, 0.9] {
        near(
            tails::beta(0.01, 1e8, BetaCoordinate::Lower(x), Tail::Lower, meter),
            1.,
            1e-14,
        );
        assert_eq!(
            tails::beta(0.01, 1e8, BetaCoordinate::Lower(x), Tail::Upper, meter),
            Ok(0.)
        );
    }
}
