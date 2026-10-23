//! Expected values follow independent amortization tables and defining equations.
use crate::FinancialError;
use crate::modern::*;
fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs() * 2e-12,
        "{actual:e} != {expected:e}"
    );
}
#[test]
fn independent_loan_and_depreciation_tables() {
    for (timing, principal, interest) in [
        (0.0, [-100.0, -110.0], [-21.0, -11.0]),
        (1.0, [-110.0, -100.0], [0.0, -10.0]),
    ] {
        for i in 0..2 {
            let p = (i + 1) as f64;
            near(
                cumulative(LoanComponent::Principal, 0.1, 2.0, 210.0, p, p, timing).unwrap(),
                principal[i],
            );
            near(
                cumulative(LoanComponent::Interest, 0.1, 2.0, 210.0, p, p, timing).unwrap(),
                interest[i],
            );
        }
        near(
            cumulative(LoanComponent::Principal, 0.1, 2.0, 210.0, 1.0, 2.0, timing).unwrap(),
            -210.0,
        );
    }
    near(
        cumulative(LoanComponent::Principal, 1e308, 2.0, 2.0, 1.0, 1.0, 1.0).unwrap(),
        -2.0,
    );
    // Tiny-rate interest approaches pv*r*(remaining fraction) rather than zero.
    near(
        cumulative(LoanComponent::Interest, 1e-20, 2.0, 210.0, 1.0, 1.0, 0.0).unwrap(),
        -2.1e-18,
    );
    near(
        cumulative(LoanComponent::Interest, 1e-20, 2.0, 210.0, 2.0, 2.0, 0.0).unwrap(),
        -1.05e-18,
    );
    near(sln(1000.0, 100.0, 3.0).unwrap(), 300.0);
    for (p, result) in [(1.0, 450.0), (2.0, 300.0), (3.0, 150.0)] {
        near(syd(1000.0, 100.0, 3.0, p).unwrap(), result);
    }
    for (p, result) in [(1.0, 250.0), (2.0, 375.0), (3.0, 93.75)] {
        near(db(1000.0, 250.0, 2.0, p, 6.0).unwrap(), result);
    }
    near(ddb(1000.0, 100.0, 5.0, 1.0, 2.0).unwrap(), 400.0);
    near(ddb(1000.0, 100.0, 5.0, 2.0, 2.0).unwrap(), 240.0);
    near(
        vdb(
            Asset {
                cost: 1000.0,
                salvage: 0.0,
                life: 5.0,
            },
            2.5,
            4.5,
            2.0,
            false,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        234.0,
    );
    near(
        vdb(
            Asset {
                cost: 1000.0,
                salvage: 0.0,
                life: 5.0,
            },
            2.5,
            4.5,
            2.0,
            true,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        184.32,
    );
    near(
        vdb(
            Asset {
                cost: 1000.0,
                salvage: 0.0,
                life: 1e12,
            },
            0.0,
            1.0,
            2.0,
            false,
            |_| Ok::<_, ()>(()),
        )
        .unwrap(),
        2e-9,
    );
    assert_eq!(
        vdb(
            Asset {
                cost: 1000.0,
                salvage: 0.0,
                life: 10.0
            },
            0.0,
            5.0,
            2.0,
            false,
            |_| Err("stop")
        ),
        Err(Failure::Operational("stop"))
    );
}
#[test]
fn stable_rates_and_complete_products() {
    near(effect(0.2, 2.9).unwrap(), 0.21);
    near(nominal(0.21, 2.9).unwrap(), 0.2);
    near(effect(1e-20, 2.0).unwrap(), 1e-20);
    near(nominal(1e-20, 2.0).unwrap(), 1e-20);
    near(rri(2.0, 100.0, 121.0).unwrap(), 0.1);
    near(pduration(0.1, 100.0, 121.0).unwrap(), 2.0);
    near(
        rri(2000.0, 1e-300, 1e300).unwrap(),
        10.0_f64.powf(0.3) - 1.0,
    );
    near(fvschedule(100.0, &[0.1, 0.0, 0.2]).unwrap(), 132.0);
    let tiny = 2.0_f64.powi(-50);
    near(
        fvschedule(1e300, &[1e20, tiny - 1.0, tiny - 1.0]).unwrap(),
        1e300 * (1e20 * tiny * tiny),
    );
    assert_eq!(sln(1.0, 0.0, 0.0), Err(FinancialError::DivZero));
    assert_eq!(effect(0.0, 2.0), Err(FinancialError::Num));
}
#[test]
fn irregular_equations_multiple_roots_and_failure() {
    let pair = [
        DatedFlow {
            value: -100.0,
            days: 0.0,
        },
        DatedFlow {
            value: 110.0,
            days: 365.0,
        },
    ];
    assert!(xnpv(0.1, &pair).unwrap().abs() < 1e-12);
    near(xirr(&pair, 0.2, |_| Ok::<_, ()>(())).unwrap(), 0.1);
    let half = [
        DatedFlow {
            value: -100.0,
            days: 0.0,
        },
        DatedFlow {
            value: 110.0,
            days: 182.5,
        },
    ];
    near(xirr(&half, 0.1, |_| Ok::<_, ()>(())).unwrap(), 0.21);
    let multiple = [
        DatedFlow {
            value: -100.0,
            days: 0.0,
        },
        DatedFlow {
            value: 230.0,
            days: 365.0,
        },
        DatedFlow {
            value: -132.0,
            days: 730.0,
        },
    ];
    near(xirr(&multiple, 0.08, |_| Ok::<_, ()>(())).unwrap(), 0.1);
    near(xirr(&multiple, 0.3, |_| Ok::<_, ()>(())).unwrap(), 0.2);
    let repeated = [
        DatedFlow {
            value: -100.0,
            days: 0.0,
        },
        DatedFlow {
            value: 200.0,
            days: 365.0,
        },
        DatedFlow {
            value: -100.0,
            days: 730.0,
        },
    ];
    assert_eq!(xirr(&repeated, 0.0, |_| Ok::<_, ()>(())), Ok(0.0));
    let no_root = [
        DatedFlow {
            value: -100.0,
            days: 0.0,
        },
        DatedFlow {
            value: 50.0,
            days: 365.0,
        },
        DatedFlow {
            value: -100.0,
            days: 730.0,
        },
    ];
    assert_eq!(
        xirr(&no_root, 0.1, |_| Ok::<_, ()>(())),
        Err(Failure::Spreadsheet(FinancialError::Num))
    );
    assert_eq!(
        xirr(&pair, 0.2, |_| Err("stop")),
        Err(Failure::Operational("stop"))
    );
}

#[test]
fn independent_high_precision_loan_intervals() {
    for (rate, periods, present, start, end, timing, principal, interest) in [
        (
            0.005,
            360.0,
            100000.0,
            350.0,
            355.0,
            0.0,
            -3448.1112374891145,
            -149.1919134274,
        ),
        (
            0.005,
            360.0,
            100000.0,
            350.0,
            355.0,
            1.0,
            -3430.956455213049,
            -148.44966510189056,
        ),
        (
            1e-12,
            1000000000.0,
            100000.0,
            999999900.0,
            999999905.0,
            0.0,
            -0.0006003000499411696,
            -5.912955492208689e-14,
        ),
        (
            1e-12,
            1000000000.0,
            100000.0,
            999999900.0,
            999999905.0,
            1.0,
            -0.0006003000499405693,
            -5.912955492202777e-14,
        ),
        (
            1e-14,
            1000000.0,
            210.0,
            1.0,
            3.0,
            0.0,
            -0.0006299999968500094,
            -6.299993700000031e-12,
        ),
        (
            1e-14,
            1000000.0,
            210.0,
            1.0,
            3.0,
            1.0,
            -0.0006299999989500031,
            -4.19999369999999e-12,
        ),
        (
            0.75,
            50.0,
            1000.0,
            45.0,
            50.0,
            0.0,
            -965.1845744552019,
            -3534.81542554797,
        ),
        (
            0.75,
            50.0,
            1000.0,
            45.0,
            50.0,
            1.0,
            -551.5340425458296,
            -2019.8945288845543,
        ),
        (
            100.0,
            100.0,
            1.0,
            95.0,
            100.0,
            0.0,
            -0.999999999999058,
            -599.0000000000009,
        ),
        (
            100.0,
            100.0,
            1.0,
            95.0,
            100.0,
            1.0,
            -0.009900990099000574,
            -5.93069306930694,
        ),
        (
            0.000999999,
            2.0,
            210.0,
            2.0,
            2.0,
            0.0,
            -105.0524737106709,
            -0.10505236865819718,
        ),
        (
            0.000999999,
            2.0,
            210.0,
            2.0,
            2.0,
            1.0,
            -104.9475262893291,
            -0.10494742134180281,
        ),
        (
            0.001000001,
            2.0,
            210.0,
            2.0,
            2.0,
            0.0,
            -105.05247381556597,
            -0.1050525788680398,
        ),
        (
            0.001000001,
            2.0,
            210.0,
            2.0,
            2.0,
            1.0,
            -104.94752618443403,
            -0.1049476311319602,
        ),
        (
            0.00099,
            100000.0,
            210.0,
            99998.0,
            100000.0,
            0.0,
            -0.6224671086062109,
            -0.0012328913937890943,
        ),
        (
            0.00099,
            100000.0,
            210.0,
            99998.0,
            100000.0,
            1.0,
            -0.6218514756453221,
            -0.001231672038471008,
        ),
    ] {
        near(
            cumulative(
                LoanComponent::Principal,
                rate,
                periods,
                present,
                start,
                end,
                timing,
            )
            .unwrap(),
            principal,
        );
        near(
            cumulative(
                LoanComponent::Interest,
                rate,
                periods,
                present,
                start,
                end,
                timing,
            )
            .unwrap(),
            interest,
        );
    }
}

#[test]
fn cumulative_totals_recover_finite_extreme_products() {
    // Independently calculated with Decimal at 240 digits: the full principal
    // is returned, and interest equals principal minus the total loan payments.
    near(
        cumulative(
            LoanComponent::Interest,
            1e-20,
            1e20,
            f64::MAX,
            1.0,
            1e20,
            0.0,
        )
        .unwrap(),
        -1.0462155305887664e308,
    );
    assert_eq!(
        cumulative(LoanComponent::Interest, 1e-20, 1e20, 1e-323, 1.0, 1e20, 0.0).unwrap(),
        -5e-324
    );
}

#[test]
fn huge_payment_counts_preserve_beginning_offsets() {
    for (timing, last_principal, last_interest) in [
        (0.0, -9.090909090909092, -0.9090909090909091),
        (1.0, -8.264462809917356, -0.8264462809917356),
    ] {
        near(
            cumulative(
                LoanComponent::Principal,
                0.1,
                1e20,
                100.0,
                1.0,
                1e20,
                timing,
            )
            .unwrap(),
            -100.0,
        );
        near(
            cumulative(
                LoanComponent::Principal,
                0.1,
                1e20,
                100.0,
                1e20,
                1e20,
                timing,
            )
            .unwrap(),
            last_principal,
        );
        near(
            cumulative(
                LoanComponent::Interest,
                0.1,
                1e20,
                100.0,
                1e20,
                1e20,
                timing,
            )
            .unwrap(),
            last_interest,
        );
    }
}
#[test]
fn zero_rate_discounting_preserves_cancellation_and_linear_magnitude() {
    let flows = [
        DatedFlow {
            value: -1e308,
            days: 0.0,
        },
        DatedFlow {
            value: 1.0,
            days: 365.0,
        },
        DatedFlow {
            value: 1e308,
            days: 730.0,
        },
    ];
    near(xnpv(0.0, &flows).unwrap(), 1.0);
    near(
        xnpv(
            0.0,
            &[DatedFlow {
                value: f64::MAX,
                days: 0.0,
            }],
        )
        .unwrap(),
        f64::MAX,
    );
}

#[test]
fn smallest_rate_preserves_representable_loan_interest() {
    let rate = f64::from_bits(1);
    let interest = -f64::MAX * rate;
    near(
        cumulative(LoanComponent::Interest, rate, 1.0, f64::MAX, 1.0, 1.0, 0.0).unwrap(),
        interest,
    );
    assert_eq!(
        cumulative(LoanComponent::Interest, rate, 1.0, f64::MAX, 1.0, 1.0, 1.0).unwrap(),
        0.0
    );
    near(
        cumulative(LoanComponent::Interest, rate, 2.0, f64::MAX, 2.0, 2.0, 1.0).unwrap(),
        interest / 2.0,
    );
}

#[test]
fn fractional_life_uses_finite_interval_contributions() {
    let asset = Asset {
        cost: f64::MAX,
        salvage: 0.0,
        life: 0.5,
    };
    near(
        vdb(asset, 0.0, 0.5, 2.0, false, |_| Ok::<_, ()>(())).unwrap(),
        f64::MAX,
    );
    near(
        vdb(asset, 0.1, 0.4, 2.0, false, |_| Ok::<_, ()>(())).unwrap(),
        f64::MAX * 0.6,
    );
    near(
        vdb(asset, 0.0, 0.5, 2.0, true, |_| Ok::<_, ()>(())).unwrap(),
        f64::MAX * 0.5,
    );
}
