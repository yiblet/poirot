//! Independent calendar and cash-flow controls; no upstream test fixtures.
use super::FinancialError;
use super::securities::*;
use crate::coercion::{
    date_time::CalendarEpoch,
    dates::calendar::{self, Date},
};
fn day(y: i64, m: i64, d: i64) -> Day {
    let serial = calendar::epoch(Date { y, m, d })
        - calendar::epoch(Date {
            y: 1899,
            m: 12,
            d: 31,
        });
    Day::from_serial(
        (serial + i64::from(serial >= 60)) as f64,
        CalendarEpoch::Excel1900,
    )
    .unwrap()
}
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 3e-12 + 1e-13,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn maturity_anchoring_preserves_month_ends_and_coupon_boundaries() {
    for (settlement, maturity, previous, next, count) in [
        (
            (2024, 2, 29),
            (2026, 8, 31),
            (2024, 2, 29),
            (2024, 8, 31),
            5,
        ),
        (
            (2024, 2, 28),
            (2026, 8, 31),
            (2023, 8, 31),
            (2024, 2, 29),
            6,
        ),
        ((2023, 3, 1), (2024, 8, 30), (2023, 2, 28), (2023, 8, 30), 3),
        ((2099, 9, 1), (2101, 8, 31), (2099, 8, 31), (2100, 2, 28), 4),
    ] {
        let p = coupon_period(
            day(settlement.0, settlement.1, settlement.2),
            day(maturity.0, maturity.1, maturity.2),
            Frequency::Semiannual,
            Basis::Actual,
        )
        .unwrap();
        assert_eq!(
            (p.previous, p.next, p.count),
            (
                day(previous.0, previous.1, previous.2),
                day(next.0, next.1, next.2),
                count
            )
        );
    }
}
#[test]
fn basis_numerators_and_coupon_denominators_remain_distinct() {
    let s = day(2024, 3, 15);
    let m = day(2024, 8, 31);
    for (basis, a, remaining, e) in [
        (Basis::Nasd, 15., 166., 180.),
        (Basis::European, 16., 165., 180.),
        (Basis::Actual, 15., 169., 184.),
        (Basis::Actual360, 15., 169., 180.),
        (Basis::Actual365, 15., 169., 182.5),
    ] {
        let p = coupon_period(s, m, Frequency::Semiannual, basis).unwrap();
        assert_eq!((p.accrued, p.remaining, p.denominator), (a, remaining, e));
    }
    assert_eq!(days(day(2024, 2, 29), day(2024, 3, 31), Basis::Nasd), 31.);
    assert_eq!(
        days(day(2024, 2, 29), day(2024, 3, 31), Basis::European),
        31.
    );
}
#[test]
fn worksheet_epochs_and_fictitious_leap_slot_are_explicit() {
    let a = Day::from_serial(59., CalendarEpoch::Excel1900).unwrap();
    let b = Day::from_serial(60., CalendarEpoch::Excel1900).unwrap();
    let c = Day::from_serial(61., CalendarEpoch::Excel1900).unwrap();
    assert_eq!(days(a, b, Basis::Actual), 1.);
    assert_eq!(days(b, c, Basis::Actual), 1.);
    assert_eq!(
        Day::from_serial(0., CalendarEpoch::Excel1904).unwrap(),
        day(1904, 1, 1)
    );
    assert_eq!(day(1904, 1, 1).serial(CalendarEpoch::Excel1900), 1462.);
    assert!(Day::from_serial(-1., CalendarEpoch::Excel1900).is_err());
    assert!(Day::from_serial(2957004., CalendarEpoch::Excel1904).is_err());
}
#[test]
fn regular_bond_controls_come_from_explicit_discounted_payments() {
    let bond = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        0.1,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(bond.price(0.1).unwrap(), 100.);
    near(bond.duration(0.1).unwrap(), 21. / 11.);
    near(bond.modified_duration(0.1).unwrap(), 210. / 121.);
    near(bond.yield_rate(100., |_| Ok::<_, ()>(())).unwrap(), 0.1);
    near(bond.price(0.).unwrap(), 120.);
    let one = Bond::new(
        day(2024, 7, 15),
        day(2025, 1, 15),
        0.1,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(one.price(0.1).unwrap(), 110. / 1.05 - 5.);
    near(
        one.yield_rate(110. / 1.05 - 5., |_| Ok::<_, ()>(()))
            .unwrap(),
        0.1,
    );
}
#[test]
fn prices_and_yields_cover_zero_tiny_large_and_negative_roots() {
    for f in [
        Frequency::Annual,
        Frequency::Semiannual,
        Frequency::Quarterly,
    ] {
        for basis in [
            Basis::Nasd,
            Basis::Actual,
            Basis::Actual360,
            Basis::Actual365,
            Basis::European,
        ] {
            let bond = Bond::new(day(2024, 7, 15), day(2050, 7, 15), 0.03, 100., f, basis).unwrap();
            for rate in [0., 1e-12, 0.01, 0.5, 2., 100.] {
                let price = bond.price(rate).unwrap();
                near(
                    bond.yield_rate(price, |_| Ok::<_, ()>(()))
                        .unwrap_or_else(|e| panic!("{f:?} {basis:?} {rate} {price}: {e:?}")),
                    rate,
                );
            }
            assert!(bond.yield_rate(1000., |_| Ok::<_, ()>(())).unwrap() < 0.);
        }
    }
}
#[test]
fn duration_is_stable_when_long_maturity_redemption_underflows() {
    let bond = Bond::new(
        day(1900, 1, 1),
        day(9999, 1, 1),
        0.1,
        100.,
        Frequency::Quarterly,
        Basis::Nasd,
    )
    .unwrap();
    near(bond.duration(4.).unwrap(), 0.5); // geometric perpetuity: q=.5, mean payment index2, four/year.
    near(bond.modified_duration(4.).unwrap(), 0.25);
    let mut work = 0;
    assert!(
        bond.yield_rate(1., |n| {
            work += n;
            Ok::<_, ()>(())
        })
        .is_ok()
    );
    assert!(work <= 3200);
}
#[test]
fn accrued_interest_and_discounted_security_equations_are_independent() {
    let (s, m) = (day(2024, 1, 15), day(2024, 7, 15));
    near(
        accrued_at_maturity(s, day(2024, 4, 15), 0.1, 1000., Basis::Nasd).unwrap(),
        25.,
    );
    for basis in [Basis::Nasd, Basis::European] {
        near(
            accrued_interest(
                s,
                m,
                day(2024, 4, 15),
                0.1,
                1000.,
                Frequency::Semiannual,
                basis,
                true,
                |_| Ok::<_, ()>(()),
            )
            .unwrap(),
            25.,
        );
        near(
            accrued_interest(
                s,
                m,
                day(2025, 1, 15),
                0.1,
                1000.,
                Frequency::Semiannual,
                basis,
                false,
                |_| Ok::<_, ()>(()),
            )
            .unwrap(),
            50.,
        );
        near(
            discount(s, m, 0.1, 100., basis, DiscountOperation::Price).unwrap(),
            95.,
        );
        near(
            discount(s, m, 95., 100., basis, DiscountOperation::Discount).unwrap(),
            0.1,
        );
        near(
            discount(s, m, 95., 100., basis, DiscountOperation::Yield).unwrap(),
            2. / 19.,
        );
        near(
            discount(s, m, 95., 0.1, basis, DiscountOperation::Received).unwrap(),
            100.,
        );
    }
    let first = day(2024, 8, 31);
    near(
        accrued_interest(
            day(2024, 2, 29),
            first,
            day(2025, 2, 28),
            0.1,
            1000.,
            Frequency::Semiannual,
            Basis::Actual,
            true,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        100.,
    );
}
#[test]
fn maturity_interest_price_uses_issue_accrual_separately() {
    let (issue, s, m) = (day(2024, 1, 15), day(2024, 4, 15), day(2024, 7, 15));
    let price = 105. / 1.0125 - 2.5;
    near(
        maturity_security(
            s,
            m,
            issue,
            0.1,
            0.05,
            Basis::Nasd,
            MaturityOperation::Price,
        )
        .unwrap(),
        price,
    );
    near(
        maturity_security(
            s,
            m,
            issue,
            0.1,
            price,
            Basis::Nasd,
            MaturityOperation::Yield,
        )
        .unwrap(),
        0.05,
    );
}
#[test]
fn treasury_limits_and_documented_direct_equations() {
    let (s, m) = (day(2024, 1, 15), day(2024, 7, 13)); // 180 actual days.
    near(treasury(s, m, 0.1, TreasuryOperation::Price).unwrap(), 95.);
    near(
        treasury(s, m, 95., TreasuryOperation::Yield).unwrap(),
        2. / 19.,
    );
    near(
        treasury(s, m, 0.1, TreasuryOperation::Equivalent).unwrap(),
        36.5 / 342.,
    );
    near(treasury(s, s, 0.1, TreasuryOperation::Price).unwrap(), 100.);
    assert_eq!(
        treasury(s, s, 95., TreasuryOperation::Yield),
        Err(FinancialError::Num)
    );
    assert!(
        treasury(
            day(2024, 1, 15),
            day(2025, 1, 15),
            0.1,
            TreasuryOperation::Price
        )
        .is_ok()
    );
    assert!(
        treasury(
            day(2024, 1, 15),
            day(2025, 1, 16),
            0.1,
            TreasuryOperation::Price
        )
        .is_err()
    );
}

#[test]
fn zero_coupon_inversion_brackets_large_yields_and_underflow_has_duration() {
    let bond = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        0.,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(bond.yield_rate(4., |_| Ok::<_, ()>(())).unwrap(), 4.);
    let long = Bond::new(
        day(2024, 1, 15),
        day(9999, 1, 15),
        0.,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    assert_eq!(long.price(1.).unwrap(), 0.);
    near(long.duration(1.).unwrap(), 7975.);
    let large = Bond::new(
        day(2024, 12, 15),
        day(2025, 1, 15),
        1e307,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(large.price(0.).unwrap(), 1e307 * (100. / 12.));
}

#[test]
fn scaled_money_preserves_subnormal_coupons_and_large_discount_equations() {
    let small = f64::from_bits(1);
    let bond = Bond::new(
        day(2024, 1, 15),
        day(2024, 4, 15),
        small,
        small,
        Frequency::Quarterly,
        Basis::Nasd,
    )
    .unwrap();
    assert_eq!(bond.price(0.).unwrap(), f64::from_bits(26));
    let zero = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        0.,
        small,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(zero.yield_rate(small, |_| Ok::<_, ()>(())).unwrap(), 0.);
    near(
        treasury(
            day(2024, 1, 15),
            day(2024, 7, 13),
            1e308,
            TreasuryOperation::Equivalent,
        )
        .unwrap(),
        -365. / 180.,
    );
    assert_eq!(
        treasury(
            day(2024, 1, 15),
            day(2024, 7, 13),
            small,
            TreasuryOperation::Equivalent
        )
        .unwrap(),
        small
    );
    assert!(
        treasury(
            day(2023, 2, 28),
            day(2024, 2, 29),
            0.1,
            TreasuryOperation::Price
        )
        .is_err()
    );
}

#[test]
fn maturity_price_ratio_avoids_overflowing_both_interest_totals() {
    let (s, m) = (day(2024, 1, 15), day(2050, 1, 15));
    near(
        maturity_security(s, m, s, 1e308, 1e308, Basis::Nasd, MaturityOperation::Price).unwrap(),
        100.,
    );
    near(
        maturity_security(s, m, s, 1e308, 100., Basis::Nasd, MaturityOperation::Yield).unwrap(),
        1e308,
    );
}

#[test]
fn long_term_investment_yield_keeps_a_representable_scaled_ratio() {
    let (s, m) = (day(1900, 1, 15), day(9999, 1, 15));
    near(
        discount(s, m, 0.01, 1e308, Basis::Nasd, DiscountOperation::Yield).unwrap(),
        1e308 / (0.01 * 8099.),
    );
}

#[test]
fn nasd_accrual_splits_nonadditive_february_fragments_and_charges_work() {
    let (issue, first, settlement) = (day(2022, 8, 28), day(2023, 8, 28), day(2023, 8, 28));
    let mut work = 0;
    // First quasi interval180/180; February28 becomes30 at the second start, giving178/180.
    near(
        accrued_interest(
            issue,
            first,
            settlement,
            0.1,
            1000.,
            Frequency::Semiannual,
            Basis::Nasd,
            true,
            |n| {
                work += n;
                Ok::<_, ()>(())
            },
        )
        .unwrap(),
        50. * (1. + 178. / 180.),
    );
    assert_eq!(work, 16);
    // A non-EOM29 anchor clamps only February; directed overlaps are149+179 days.
    near(
        accrued_interest(
            day(2024, 9, 29),
            day(2025, 8, 29),
            day(2025, 8, 29),
            0.1,
            1000.,
            Frequency::Semiannual,
            Basis::Nasd,
            true,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        50. * 328. / 180.,
    );
    // A one-day overlap inside one quasi period must not subtract two NASD prefixes.
    near(
        accrued_interest(
            day(2025, 2, 28),
            day(2025, 8, 29),
            day(2025, 3, 1),
            0.1,
            1000.,
            Frequency::Annual,
            Basis::Nasd,
            true,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        100. / 360.,
    );
    assert!(matches!(
        accrued_interest(
            issue,
            first,
            settlement,
            0.1,
            1000.,
            Frequency::Semiannual,
            Basis::Nasd,
            true,
            |_| Err("budget")
        ),
        Err(super::financial_calculation_error::FinancialCalculationError::Operational("budget"))
    ));
}

#[test]
fn redemption_recovers_an_underflowed_discount_factor() {
    let bond = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        0.,
        1e308,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    // 1e308/(1+1e200)^2 is1e-92 to the precision of the supplied inputs.
    near(bond.price(1e200).unwrap() / 1e-92, 1.);
    let periodic = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        1e-200,
        1e308,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    near(periodic.price(1e200).unwrap() / 1e-92, 1.);
    near(bond.duration(1e200).unwrap(), 2.);
    let huge_coupon = Bond::new(
        day(2024, 1, 15),
        day(2026, 1, 15),
        1e308,
        100.,
        Frequency::Annual,
        Basis::Nasd,
    )
    .unwrap();
    // 100*1e308/(1+1e308) remains100 despite the coupon amount and discount scale.
    near(huge_coupon.price(1e308).unwrap(), 100.);
}

#[test]
fn final_coupon_discount_keeps_a_finite_scaled_denominator() {
    let bond = Bond::new(
        day(2023, 1, 15),
        day(2024, 1, 15),
        0.,
        100.,
        Frequency::Annual,
        Basis::Actual360,
    )
    .unwrap();
    near(
        bond.price(f64::MAX).unwrap() / (100. / f64::MAX * 360. / 365.),
        1.,
    );
}
