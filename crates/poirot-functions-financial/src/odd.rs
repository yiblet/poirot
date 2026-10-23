//! Irregular cash-flow equations and date-prorated depreciation, independently implemented.
//! Contracts and native worksheet uncertainties are recorded in ODD_SOURCES.md.
use crate::{
    FinancialError as E, Result, checked, finite,
    modern::{Failure, scaled_product, scaled_quotient},
    securities::{self as calendar, Basis, Day, Frequency},
};

/// An odd first coupon followed by regularly spaced coupon payments.
#[derive(Clone, Copy, Debug)]
pub struct OddFirst {
    first_units: f64,
    accrued: f64,
    first_time: f64,
    later: u32,
    coupon: f64,
    redemption: f64,
    frequency: Frequency,
}
impl OddFirst {
    /// Validate the date order and maturity schedule, then prepare quasi-coupon measures.
    #[allow(clippy::too_many_arguments)]
    pub fn new<O>(
        settlement: Day,
        maturity: Day,
        issue: Day,
        first: Day,
        coupon: f64,
        redemption: f64,
        frequency: Frequency,
        basis: Basis,
        mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> std::result::Result<Self, Failure<O>> {
        use Failure::Spreadsheet;
        finite(&[coupon, redemption]).map_err(Spreadsheet)?;
        if !(issue < settlement && settlement < first && first < maturity)
            || coupon < 0.0
            || redemption <= 0.0
        {
            return Err(Spreadsheet(E::Num));
        }
        let regular =
            calendar::coupon_period(first, maturity, frequency, basis).map_err(Spreadsheet)?;
        if regular.previous != first {
            return Err(Spreadsheet(E::Num));
        }
        let period =
            calendar::coupon_period(settlement, maturity, frequency, basis).map_err(Spreadsheet)?;
        Ok(Self {
            first_units: calendar::quasi_fraction(
                issue,
                first,
                maturity,
                frequency,
                basis,
                &mut charge,
            )?,
            accrued: calendar::quasi_fraction(
                issue,
                settlement,
                maturity,
                frequency,
                basis,
                &mut charge,
            )?,
            first_time: f64::from(
                period
                    .count
                    .checked_sub(regular.count)
                    .and_then(|v| v.checked_sub(1))
                    .ok_or(Spreadsheet(E::Num))?,
            ) + period.remaining / period.denominator,
            later: regular.count,
            coupon,
            redemption,
            frequency,
        })
    }
    fn factors(self, yield_rate: f64) -> Result<(f64, f64, f64, f64)> {
        let r = yield_rate / self.frequency.payments() as f64;
        if !r.is_finite() || r <= -1.0 {
            return Err(E::Num);
        }
        let log = r.ln_1p();
        let last = self.first_time + f64::from(self.later);
        let (sum, weighted) = calendar::geometric((-log.abs()).exp(), self.later);
        let tail_time = if r >= 0.0 {
            self.first_time + 1.0 + weighted / sum
        } else {
            last - weighted / sum
        };
        let tail_log = sum.ln()
            - if r >= 0.0 {
                (self.first_time + 1.0) * log
            } else {
                last * log
            };
        Ok((-self.first_time * log, tail_log, -last * log, tail_time))
    }
    fn discounted(self, yield_rate: f64) -> Result<(f64, f64)> {
        let (first, tail, last, tail_time) = self.factors(yield_rate)?;
        let f = self.frequency.payments() as f64;
        let a =
            calendar::discounted_product(&[100.0, self.coupon, 1.0 / f, self.first_units], first)?;
        let b = calendar::discounted_product(&[100.0, self.coupon, 1.0 / f], tail)?;
        let c = calendar::discounted_product(&[self.redemption], last)?;
        let value = checked(a + b + c)?;
        let weighted =
            a * self.first_time + b * tail_time + c * (self.first_time + f64::from(self.later));
        Ok((value, checked(-weighted / (f + yield_rate))?))
    }
    /// Return clean price per 100 face value for a nonnegative annual yield.
    pub fn price(self, yield_rate: f64) -> Result<f64> {
        finite(&[yield_rate])?;
        if yield_rate < 0.0 {
            return Err(E::Num);
        }
        let (first, tail, last, _) = self.factors(yield_rate)?;
        let first_log = self.first_units.ln() + first;
        let center = first_log.max(tail);
        let coupon_log = center + ((first_log - center).exp() + (tail - center).exp()).ln();
        let factor = coupon_log.exp();
        let f = self.frequency.payments() as f64;
        let coupons = if factor.is_normal() {
            // Combine before monetary multiplication so ordinary clean-price cancellation is retained.
            scaled_product([100.0, self.coupon, 1.0 / f, factor - self.accrued])?
        } else {
            calendar::discounted_product(&[100.0, self.coupon, 1.0 / f], coupon_log)?
                - scaled_product([100.0, self.coupon, 1.0 / f, self.accrued])?
        };
        checked(coupons + calendar::discounted_product(&[self.redemption], last)?)
    }
    /// Invert the odd-first discounted cash flows with at most 100 charged Newton passes.
    pub fn yield_rate<O>(
        self,
        price: f64,
        mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> std::result::Result<f64, Failure<O>> {
        use Failure::Spreadsheet;
        finite(&[price]).map_err(Spreadsheet)?;
        if price <= 0.0 {
            return Err(Spreadsheet(E::Num));
        }
        let scale = price.max(self.coupon).max(self.redemption);
        let bond = Self {
            coupon: self.coupon / scale,
            redemption: self.redemption / scale,
            ..self
        };
        let dirty = checked(
            price / scale
                + scaled_product([
                    100.0,
                    bond.coupon,
                    1.0 / self.frequency.payments() as f64,
                    self.accrued,
                ])
                .map_err(Spreadsheet)?,
        )
        .map_err(Spreadsheet)?;
        let mut lower = -(self.frequency.payments() as f64);
        let mut upper = None;
        let mut rate = 0.1_f64;
        let mut prior = f64::INFINITY;
        for _ in 0..100 {
            charge(32).map_err(Failure::Operational)?;
            let (value, derivative) = match bond.discounted(rate) {
                Ok(v) => v,
                Err(e) if rate >= 0.0 => return Err(Spreadsheet(e)),
                Err(_) => {
                    lower = rate;
                    rate = (lower + upper.ok_or(Spreadsheet(E::Num))?) * 0.5;
                    continue;
                }
            };
            let residual = value - dirty;
            if residual == 0.0
                || (residual.abs() <= 2e-12 * dirty
                    && (rate - prior).abs() <= 2e-10 * (1.0 + rate.abs()))
            {
                return Ok(rate);
            }
            if residual > 0.0 {
                lower = rate;
            } else {
                upper = Some(rate);
            }
            let Some(hi) = upper else {
                prior = rate;
                rate = checked(rate * 2.0 + 1.0).map_err(Spreadsheet)?;
                continue;
            };
            let next = rate - residual / derivative;
            prior = rate;
            rate = if next.is_finite() && next > lower && next < hi {
                next
            } else {
                (lower + hi) * 0.5
            };
        }
        Err(Spreadsheet(E::Num))
    }
}

/// The single irregular coupon interval following the last actual interest payment.
#[derive(Clone, Copy, Debug)]
pub struct OddLast {
    total: f64,
    accrued: f64,
    remaining: f64,
    coupon: f64,
    redemption: f64,
    frequency: Frequency,
}
impl OddLast {
    /// Validate dates and prepare the three distinct forward quasi-coupon measures.
    #[allow(clippy::too_many_arguments)]
    pub fn new<O>(
        settlement: Day,
        maturity: Day,
        last: Day,
        coupon: f64,
        redemption: f64,
        frequency: Frequency,
        basis: Basis,
        mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> std::result::Result<Self, Failure<O>> {
        use Failure::Spreadsheet;
        finite(&[coupon, redemption]).map_err(Spreadsheet)?;
        if !(last < settlement && settlement < maturity) || coupon < 0.0 || redemption <= 0.0 {
            return Err(Spreadsheet(E::Num));
        }
        Ok(Self {
            total: calendar::quasi_fraction(last, maturity, last, frequency, basis, &mut charge)?,
            accrued: calendar::quasi_fraction(
                last,
                settlement,
                last,
                frequency,
                basis,
                &mut charge,
            )?,
            remaining: calendar::quasi_fraction(
                settlement,
                maturity,
                last,
                frequency,
                basis,
                &mut charge,
            )?,
            coupon,
            redemption,
            frequency,
        })
    }
    /// Return clean price using simple discount over the remaining odd interval.
    pub fn price(self, yield_rate: f64) -> Result<f64> {
        finite(&[yield_rate])?;
        if yield_rate < 0.0 {
            return Err(E::Num);
        }
        let f = self.frequency.payments() as f64;
        let y = yield_rate / f;
        let (redemption, coefficient) = if y < 1.0 {
            let z = y * self.remaining;
            (
                scaled_quotient([self.redemption], [1.0 + z])?,
                (-self.accrued).mul_add(z, self.total - self.accrued) / (1.0 + z),
            )
        } else {
            let denom = 1.0 / y + self.remaining;
            (
                scaled_quotient([self.redemption], [y, denom])?,
                ((self.total - self.accrued) / y - self.accrued * self.remaining) / denom,
            )
        };
        checked(redemption + scaled_product([100.0, self.coupon, 1.0 / f, coefficient])?)
    }
    /// Solve the last-period equation directly, without iterative root finding.
    pub fn yield_rate(self, price: f64) -> Result<f64> {
        finite(&[price])?;
        if price <= 0.0 {
            return Err(E::Num);
        }
        if self.remaining == 0.0 {
            return Err(E::DivZero);
        }
        let scale = price.max(self.coupon).max(self.redemption);
        let f = self.frequency.payments() as f64;
        let coupon = scaled_quotient([100.0, self.coupon], [scale, f])?;
        let dirty = price / scale + coupon * self.accrued;
        scaled_quotient(
            [
                self.redemption / scale - price / scale + coupon * (self.total - self.accrued),
                f,
            ],
            [dirty, self.remaining],
        )
    }
}

/// Depreciation style for a date-prorated French accounting schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Amortization {
    /// Constant annual charge after the initial prorated charge.
    Linear,
    /// Whole-unit rounded declining balance with life-dependent acceleration.
    Declining,
}
/// Calculate one period without allocating a depreciation schedule.
#[allow(clippy::too_many_arguments)]
pub fn amortization<O>(
    cost: f64,
    purchased: Day,
    first: Day,
    salvage: f64,
    period: f64,
    rate: f64,
    basis: Basis,
    operation: Amortization,
    mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<f64, Failure<O>> {
    use Failure::Spreadsheet;
    finite(&[cost, salvage, period, rate]).map_err(Spreadsheet)?;
    if cost < 0.0
        || salvage < 0.0
        || salvage > cost
        || period < 0.0
        || rate <= 0.0
        || purchased > first
        || basis == Basis::Actual360
    {
        return Err(Spreadsheet(E::Num));
    }
    let period = period.trunc();
    let available = cost - salvage;
    if operation == Amortization::Linear && available == 0.0 {
        return Ok(0.0);
    }
    let capped = |factors: &[f64], cap: f64| -> Result<f64> {
        if cap == 0.0 || factors.contains(&0.0) {
            return Ok(0.0);
        }
        // All locally constructed factors are nonnegative; an overflow exceeds the finite cap.
        match scaled_product(factors.iter().copied()) {
            Ok(v) => Ok(v.min(cap)),
            Err(E::Num) => Ok(cap),
            Err(e) => Err(e),
        }
    };
    let fraction = calendar::depreciation_fraction(purchased, first, basis).map_err(Spreadsheet)?;
    if operation == Amortization::Linear {
        let initial = capped(&[cost, rate, fraction], available).map_err(Spreadsheet)?;
        if period == 0.0 {
            return Ok(initial);
        }
        let regular = capped(&[cost, rate], available).map_err(Spreadsheet)?;
        // Compare before multiplying: an enormous period is already beyond the salvage cap.
        let remaining = available - initial;
        if remaining == 0.0 || period - 1.0 >= remaining / regular {
            return Ok(0.0);
        }
        return checked(regular.min((remaining - regular * (period - 1.0)).max(0.0)))
            .map_err(Spreadsheet);
    }
    let life = 1.0 / rate;
    let coefficient = if (3.0..=4.0).contains(&life) {
        1.5
    } else if (5.0..=6.0).contains(&life) {
        2.0
    } else if life > 6.0 {
        2.5
    } else {
        return Err(Spreadsheet(E::Num));
    };
    let accelerated = rate * coefficient;
    let round = |value| poirot_functions_math::round(value, 0.0).map_err(|_| Spreadsheet(E::Num));
    let initial = round(scaled_product([cost, accelerated, fraction]).map_err(Spreadsheet)?)?;
    if period == 0.0 {
        return Ok(initial);
    }
    if cost <= initial {
        return Ok(0.0);
    }
    if round(scaled_product([cost - initial, accelerated]).map_err(Spreadsheet)?)? == 0.0 {
        return Ok(0.0);
    }
    if period > 100_000.0 {
        return Err(Failure::ComputationalLimit);
    }
    let mut book = cost - initial;
    let mut value = 0.0;
    for index in 1..=period as u32 {
        charge(4).map_err(Failure::Operational)?;
        let remaining = (book - salvage).max(0.0);
        value = round(scaled_product([book, accelerated]).map_err(Spreadsheet)?)?;
        if value > remaining {
            // Crossing the depreciable amount starts the documented terminal half-book charge.
            // The cumulative stopping condition makes subsequent periods zero.
            return Ok(if f64::from(index) == period {
                round(book * 0.5)?
            } else {
                0.0
            });
        }
        book -= value;
        if book <= salvage {
            return Ok(if f64::from(index) == period {
                value
            } else {
                0.0
            });
        }
    }
    Ok(value)
}
