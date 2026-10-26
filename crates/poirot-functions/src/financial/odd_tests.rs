//! Independently authored cash-flow and depreciation controls; no imported fixtures.
use super::{
    FinancialError,
    odd::*,
    securities::{Basis, Day, Frequency},
};
use crate::coercion::{
    date_time::CalendarEpoch,
    dates::calendar::{self, Date},
};
fn day(y: i64, m: i64, d: i64) -> Day {
    let v = calendar::epoch(Date { y, m, d }) + 25568;
    let v = v + i64::from(v >= 60);
    Day::from_serial(v as f64, CalendarEpoch::Excel1900).unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 3e-12 * expected.abs() + 1e-12,
        "{actual:e} != {expected:e}"
    );
}
fn first(issue: Day, settlement: Day, first: Day, maturity: Day, basis: Basis) -> OddFirst {
    OddFirst::new(
        settlement,
        maturity,
        issue,
        first,
        0.1,
        100.,
        Frequency::Semiannual,
        basis,
        |_| Ok::<_, ()>(()),
    )
    .unwrap()
}
#[test]
fn first_coupon_short_and_long_have_independent_cash_flows() {
    let short = OddFirst::new(
        day(2024, 5, 1),
        day(2025, 1, 1),
        day(2024, 4, 1),
        day(2024, 7, 1),
        0.12,
        100.,
        Frequency::Semiannual,
        Basis::Nasd,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    near(short.price(0.).unwrap(), 108.);
    near(
        short.price(0.1).unwrap(),
        3. / 1.05_f64.powf(1. / 3.) + 106. / 1.05_f64.powf(4. / 3.) - 1.,
    );
    let long = first(
        day(2023, 1, 1),
        day(2023, 4, 1),
        day(2024, 1, 1),
        day(2025, 1, 1),
        Basis::Nasd,
    );
    // Authored payment stream:10 at1.5,5 at2.5,105 at3.5; accrued2.5.
    near(long.price(0.1).unwrap(), 99.73715049937014);
    near(
        long.yield_rate(99.73715049937014, |_| Ok::<_, ()>(()))
            .unwrap(),
        0.1,
    );
    near(long.price(0.).unwrap(), 117.5);
}
#[test]
fn maturity_anchor_survives_a_clamped_first_coupon() {
    // Aug30 maturity clamps to Feb29, but the next August boundary remains Aug30.
    let bond = first(
        day(2023, 8, 30),
        day(2023, 11, 30),
        day(2024, 2, 29),
        day(2024, 8, 30),
        Basis::Actual,
    );
    let initial = 183.; // Aug30→Feb29:183 actual days.
    let accrued = 92.; // Aug30→Nov30:92 days.
    let t = 91. / initial;
    near(
        bond.price(0.1).unwrap(),
        5. / 1.05_f64.powf(t) + 105. / 1.05_f64.powf(t + 1.) - 5. * accrued / initial,
    );
}
#[test]
fn last_coupon_short_long_and_leap_denominators() {
    let long = OddLast::new(
        day(2024, 4, 1),
        day(2024, 10, 1),
        day(2024, 1, 1),
        0.1,
        100.,
        Frequency::Semiannual,
        Basis::Nasd,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    near(long.price(0.1).unwrap(), 4195. / 42.);
    near(long.yield_rate(4195. / 42.).unwrap(), 0.1);
    let short = OddLast::new(
        day(2024, 2, 1),
        day(2024, 4, 1),
        day(2024, 1, 1),
        0.1,
        100.,
        Frequency::Semiannual,
        Basis::Actual,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    // Containing Jan1→Jul1 period has182 days:91 total,31 accrued,60 remaining.
    near(
        short.price(0.1).unwrap(),
        (100. + 5. * 91. / 182.) / (1. + 0.05 * 60. / 182.) - 5. * 31. / 182.,
    );
    near(short.yield_rate(short.price(0.1).unwrap()).unwrap(), 0.1);
}
#[test]
fn quasi_counts_and_numeric_scales_remain_bounded() {
    let bond = first(
        day(1900, 1, 1),
        day(1900, 4, 1),
        day(9998, 1, 1),
        day(9999, 1, 1),
        Basis::Actual,
    );
    assert!(bond.price(0.1).unwrap().is_finite());
    let huge = OddLast::new(
        day(2024, 4, 1),
        day(2024, 10, 1),
        day(2024, 1, 1),
        0.,
        1e308,
        Frequency::Semiannual,
        Basis::Nasd,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    near(huge.price(1e200).unwrap(), 2e108);
    let mut passes = 0;
    let result = first(
        day(2023, 1, 1),
        day(2023, 4, 1),
        day(2024, 1, 1),
        day(2025, 1, 1),
        Basis::Actual,
    )
    .yield_rate(50., |_| {
        passes += 1;
        Ok::<_, ()>(())
    });
    assert!(result.is_ok());
    assert!(passes <= 100);
    assert!(matches!(
        first(
            day(2023, 1, 1),
            day(2023, 4, 1),
            day(2024, 1, 1),
            day(2025, 1, 1),
            Basis::Actual
        )
        .yield_rate(50., |_| Err("budget")),
        Err(super::modern::Failure::Operational("budget"))
    ));
}
#[test]
fn security_errors_and_required_schedule_are_explicit() {
    assert!(matches!(
        OddFirst::new(
            day(2024, 5, 1),
            day(2025, 1, 2),
            day(2024, 4, 1),
            day(2024, 7, 1),
            0.1,
            100.,
            Frequency::Semiannual,
            Basis::Nasd,
            |_| Ok::<_, ()>(())
        ),
        Err(super::modern::Failure::Spreadsheet(FinancialError::Num))
    ));
    let last = OddLast::new(
        day(2024, 4, 1),
        day(2024, 10, 1),
        day(2024, 1, 1),
        0.1,
        100.,
        Frequency::Semiannual,
        Basis::Nasd,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert_eq!(last.price(-0.1), Err(FinancialError::Num));
    assert_eq!(last.yield_rate(0.), Err(FinancialError::Num));
    near(last.yield_rate(200.).unwrap(), (107.5 / 202.5 - 1.) * 2.);
}
fn amor(period: f64, operation: Amortization) -> f64 {
    amortization(
        1000.,
        day(2024, 1, 1),
        day(2024, 4, 1),
        100.,
        period,
        0.2,
        Basis::Nasd,
        operation,
        |_| Ok::<_, ()>(()),
    )
    .unwrap()
}
#[test]
fn linear_prorated_charge_full_charges_and_final_remainder() {
    for (p, v) in [
        (0., 50.),
        (1., 200.),
        (2., 200.),
        (3., 200.),
        (4., 200.),
        (5., 50.),
        (6., 0.),
        (1e300, 0.),
    ] {
        near(amor(p, Amortization::Linear), v);
    }
    near(amor(1.9, Amortization::Linear), 200.);
    // The depreciation convention adjusts both February dates, unlike ordered NASD coupons.
    near(
        amortization(
            1200.,
            day(2024, 2, 28),
            day(2024, 3, 30),
            0.,
            0.,
            0.1,
            Basis::Nasd,
            Amortization::Linear,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        10.,
    );
}
#[test]
fn declining_periods_round_balance_and_stop_after_the_terminal_crossing() {
    // Independent recurrence: initial100; .4*900=360; .4*540=216;
    // .4*324=129.6 rounds130; .4*194=77.6 rounds78.
    // Next .4*116=46.4 exceeds remaining16, so terminal half-book charge58.
    for (p, v) in [
        (0., 100.),
        (1., 360.),
        (2., 216.),
        (3., 130.),
        (4., 78.),
        (5., 58.),
        (6., 0.),
    ] {
        near(amor(p, Amortization::Declining), v);
    }
    for (rate, factor) in [(1. / 3.5, 1.5), (1. / 5.5, 2.), (0.1, 2.5)] {
        let value = amortization(
            1000.,
            day(2023, 1, 1),
            day(2024, 1, 1),
            0.,
            0.,
            rate,
            Basis::Nasd,
            Amortization::Declining,
            |_| Ok::<_, ()>(()),
        )
        .unwrap();
        near(value, (1000. * rate * factor).round());
    }
    for rate in [0.5, 1. / 4.5] {
        assert!(matches!(
            amortization(
                1000.,
                day(2023, 1, 1),
                day(2024, 1, 1),
                0.,
                0.,
                rate,
                Basis::Nasd,
                Amortization::Declining,
                |_| Ok::<_, ()>(())
            ),
            Err(super::modern::Failure::Spreadsheet(FinancialError::Num))
        ));
    }
}

#[test]
fn capped_charges_century_gaps_and_subnormal_money_are_preserved() {
    near(
        amortization(
            f64::MAX,
            day(2024, 1, 1),
            day(2025, 1, 1),
            0.,
            0.,
            2.,
            Basis::Nasd,
            Amortization::Linear,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        f64::MAX,
    );
    near(
        amortization(
            f64::MAX,
            day(2024, 1, 1),
            day(2024, 1, 1),
            0.,
            1.,
            2.,
            Basis::Nasd,
            Amortization::Linear,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        f64::MAX,
    );
    // 2100 is not leap; the next leap day after2096 occurs in2104.
    near(
        super::securities::depreciation_fraction(day(2097, 1, 1), day(2102, 1, 1), Basis::Actual)
            .unwrap(),
        5.,
    );
    let tiny = f64::from_bits(1);
    let bond = OddLast::new(
        day(2024, 4, 1),
        day(2024, 10, 1),
        day(2024, 1, 1),
        0.,
        tiny,
        Frequency::Semiannual,
        Basis::Actual,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    near(bond.yield_rate(tiny).unwrap(), 0.);
    let bond = OddLast::new(
        day(1900, 4, 1),
        day(9999, 10, 1),
        day(1900, 1, 1),
        0.,
        1e308,
        Frequency::Annual,
        Basis::Actual,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    assert!(bond.price(1e308).unwrap() > 0.0001);
}

#[test]
fn odd_first_discount_retains_money_after_coefficient_underflow() {
    let bond = OddFirst::new(
        day(1900, 1, 31),
        day(9999, 1, 30),
        day(1900, 1, 30),
        day(9998, 1, 30),
        1e308,
        100.,
        Frequency::Annual,
        Basis::Nasd,
        |_| Ok::<_, ()>(()),
    )
    .unwrap();
    // NASD accrued days are zero; the very late first coupon still has representable discounted money.
    let t = 8098.;
    let expected = (100_f64.ln() + 1e308_f64.ln() + 8098_f64.ln() - t * 1.1_f64.ln()).exp()
        + (100_f64.ln() + 1e308_f64.ln() - (t + 1.) * 1.1_f64.ln()).exp();
    let actual = bond.price(0.1).unwrap();
    assert!(
        actual > 0.0,
        "actual={actual:e},expected={expected:e},bond={bond:?}"
    );
    assert!(
        (actual / expected - 1.).abs() < 2e-12,
        "{actual:e} != {expected:e}"
    );
}

#[test]
fn declining_initial_and_terminal_charges_are_distinct_from_linear_salvage_caps() {
    // Independently stated schedule: initial .25*.4*1000=100; book900.
    // Its ordinary charge360 exceeds remaining.4, so the terminal charge is half of900.
    for (period, expected) in [(0., 100.), (1., 450.), (2., 0.)] {
        near(
            amortization(
                1000.,
                day(2024, 1, 1),
                day(2024, 4, 1),
                899.6,
                period,
                0.2,
                Basis::Nasd,
                Amortization::Declining,
                |_| Ok::<_, ()>(()),
            )
            .unwrap(),
            expected,
        );
    }
}
