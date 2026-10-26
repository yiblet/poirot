//! Security-specific calendars and independently implemented documented bond equations.
//! Primary contracts and unresolved native details are in SECURITY_SOURCES.md.
use crate::coercion::{
    date_time::CalendarEpoch,
    dates::calendar::{self, Date},
};
use crate::financial::{
    FinancialError as E, Result, checked, finite,
    modern::{scaled_product, scaled_quotient},
};

/// Day-count convention for security numerators and annual denominators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// US NASD thirty-day months and a 360-day year.
    Nasd,
    /// Actual elapsed days and actual calendar years or coupon periods.
    Actual,
    /// Actual elapsed days and a 360-day year.
    Actual360,
    /// Actual elapsed days and a 365-day year.
    Actual365,
    /// European thirty-day months and a 360-day year.
    European,
}
impl Basis {
    /// Admit a finite worksheet basis after integer truncation.
    pub fn from_number(value: f64) -> Result<Self> {
        finite(&[value])?;
        match value.trunc() {
            0.0 => Ok(Self::Nasd),
            1.0 => Ok(Self::Actual),
            2.0 => Ok(Self::Actual360),
            3.0 => Ok(Self::Actual365),
            4.0 => Ok(Self::European),
            _ => Err(E::Num),
        }
    }
}
/// Number of regularly spaced coupons paid each year.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frequency {
    /// One annual payment.
    Annual,
    /// Two semiannual payments.
    Semiannual,
    /// Four quarterly payments.
    Quarterly,
}
impl Frequency {
    /// Admit a finite worksheet frequency after integer truncation.
    pub fn from_number(value: f64) -> Result<Self> {
        finite(&[value])?;
        match value.trunc() {
            1.0 => Ok(Self::Annual),
            2.0 => Ok(Self::Semiannual),
            4.0 => Ok(Self::Quarterly),
            _ => Err(E::Num),
        }
    }
    /// Return the coupon count per year.
    pub fn payments(self) -> i64 {
        match self {
            Self::Annual => 1,
            Self::Semiannual => 2,
            Self::Quarterly => 4,
        }
    }
}
/// Admitted worksheet day with an epoch-independent serial including the 1900 leap slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day {
    serial: i32,
}
impl Day {
    /// Admit and truncate a worksheet serial without losing its fictitious leap day.
    pub fn from_serial(serial: f64, epoch: CalendarEpoch) -> Result<Self> {
        if !serial.is_finite() || serial < 0.0 {
            return Err(E::Value);
        }
        let offset = if matches!(epoch, CalendarEpoch::Excel1904) {
            1462.0
        } else {
            0.0
        };
        if serial.trunc() + offset > 2_958_465.0 {
            return Err(E::Value);
        }
        Ok(Self {
            serial: (serial.trunc() + offset) as i32,
        })
    }
    /// Return the date's serial in the caller's workbook epoch.
    pub fn serial(self, epoch: CalendarEpoch) -> f64 {
        f64::from(self.serial)
            - if matches!(epoch, CalendarEpoch::Excel1904) {
                1462.0
            } else {
                0.0
            }
    }
    fn civil(self) -> Date {
        if self.serial == 60 {
            return Date {
                y: 1900,
                m: 2,
                d: 29,
            };
        }
        // SAFETY: widening an i32 day and subtracting at most 1 plus the literal epoch
        // (-25568) stays within i64; the Gregorian algorithm's bounded products also fit.
        calendar::civil(i64::from(self.serial) - i64::from(self.serial > 60) - 25568)
    }

    fn from_civil(date: Date) -> Result<Self> {
        if !(-10000..=11000).contains(&date.y)
            || !(1..=12).contains(&date.m)
            || !(1..=31).contains(&date.d)
        {
            return Err(E::Value);
        }
        if date.y == 1900 && date.m == 2 && date.d == 29 {
            return Ok(Self { serial: 60 });
        }
        // SAFETY: the local component bounds keep every Gregorian-era product below 10 million.
        let serial = calendar::epoch(date) + 25568;
        // SAFETY: the bounded epoch above plus the single leap slot fits i64.
        let serial = serial + i64::from(serial >= 60);
        Ok(Self {
            serial: i32::try_from(serial).map_err(|_| E::Value)?,
        })
    }
}
fn month_days(year: i64, month: i64) -> i64 {
    if year == 1900 && month == 2 {
        29
    } else {
        calendar::month_days(year, month)
    }
}
fn end_of_month(date: Date) -> bool {
    date.d == month_days(date.y, date.m)
}
fn month_index(date: Date) -> Result<i64> {
    date.y
        .checked_mul(12)
        .and_then(|v| v.checked_add(date.m))
        .and_then(|v| v.checked_sub(1))
        .ok_or(E::Value)
}
/// Shift from the original coupon anchor while preserving its month-end intent.
fn shift(anchor: Day, months: i64) -> Result<Day> {
    let date = anchor.civil();
    let month = month_index(date)?.checked_add(months).ok_or(E::Value)?;
    // SAFETY: the nonzero literal divisor cannot divide i64::MIN by -1; remainder is 0..11.
    let (y, m) = (month.div_euclid(12), month.rem_euclid(12) + 1);
    Day::from_civil(Date {
        y,
        m,
        d: if end_of_month(date) {
            month_days(y, m)
        } else {
            date.d.min(month_days(y, m))
        },
    })
}

/// Count directed days using the selected security convention.
pub fn days(start: Day, end: Day, basis: Basis) -> f64 {
    if start == end {
        return 0.0;
    }
    if matches!(basis, Basis::Actual | Basis::Actual360 | Basis::Actual365) {
        return f64::from(end.serial) - f64::from(start.serial);
    }
    let (a, b) = (start.civil(), end.civil());
    let (mut d1, mut d2) = (a.d, b.d);
    if basis == Basis::European {
        d1 = d1.min(30);
        d2 = d2.min(30);
    } else if d1 == 31 && d2 == 31 {
        d1 = 30;
        d2 = 30;
    } else if d1 == 31 {
        d1 = 30;
    } else if d1 == 30 && d2 == 31 {
        d2 = 30;
    } else if a.m == 2 && end_of_month(a) {
        d1 = 30;
        if b.m == 2 && end_of_month(b) {
            d2 = 30;
        }
    }
    (b.y as f64 - a.y as f64) * 360.0 + (b.m as f64 - a.m as f64) * 30.0 + (d2 as f64 - d1 as f64)
}
fn year_days(start: Day, end: Day, basis: Basis) -> Result<f64> {
    Ok(match basis {
        Basis::Nasd | Basis::European | Basis::Actual360 => 360.0,
        Basis::Actual365 => 365.0,
        Basis::Actual => {
            let (a, b) = (start.civil(), end.civil());
            if a.y == b.y {
                return Ok(365.0 + f64::from(calendar::leap(a.y) || a.y == 1900));
            }
            if Some(b.y) == a.y.checked_add(1) && (a.m, a.d) >= (b.m, b.d) {
                let feb = Day::from_civil(Date {
                    y: if a.m <= 2 { a.y } else { b.y },
                    m: 2,
                    d: 29,
                })?;
                return Ok(
                    if (calendar::leap(feb.civil().y) || feb.civil().y == 1900)
                        && start <= feb
                        && feb <= end
                    {
                        366.0
                    } else {
                        365.0
                    },
                );
            }
            let first = Day::from_civil(Date { y: a.y, m: 1, d: 1 })?;
            let after = Day::from_civil(Date {
                y: b.y.checked_add(1).ok_or(E::Value)?,
                m: 1,
                d: 1,
            })?;
            (f64::from(after.serial) - f64::from(first.serial)) / (b.y as f64 - a.y as f64 + 1.0)
        }
    })
}
/// Measure a security year fraction using its directed day convention.
fn fraction(start: Day, end: Day, basis: Basis) -> Result<f64> {
    Ok(days(start, end, basis) / year_days(start, end, basis)?)
}
/// Settlement's position in a regular coupon schedule, used by coupon and bond calculations.
/// Accrued days, remaining days, and the basis-specific denominator stay separate because they need not add up under every day-count convention.
#[derive(Clone, Copy, Debug)]
pub struct CouponPeriod {
    /// Last coupon boundary at or before settlement.
    pub previous: Day,
    /// First coupon boundary strictly after settlement.
    pub next: Day,
    /// Number of remaining coupons through maturity.
    pub count: u32,
    /// Basis-specific days accrued since the preceding coupon.
    pub accrued: f64,
    /// Basis-specific days until the next coupon.
    pub remaining: f64,
    /// Coupon-period denominator, distinct from numerator day counts.
    pub denominator: f64,
}
/// Seek the settlement period arithmetically from the original maturity anchor.
pub fn coupon_period(
    settlement: Day,
    maturity: Day,
    frequency: Frequency,
    basis: Basis,
) -> Result<CouponPeriod> {
    if settlement >= maturity {
        return Err(E::Num);
    }
    let step = match frequency {
        Frequency::Annual => 12,
        Frequency::Semiannual => 6,
        Frequency::Quarterly => 3,
    };
    let (a, b) = (settlement.civil(), maturity.civil());
    let distance = month_index(b)?
        .checked_sub(month_index(a)?)
        .ok_or(E::Value)?;
    // SAFETY: step is locally one of 3, 6 or 12, so Euclidean division cannot panic.
    let mut count = distance.div_euclid(step);
    if shift(
        maturity,
        count
            .checked_mul(step)
            .and_then(i64::checked_neg)
            .ok_or(E::Value)?,
    )? > settlement
    {
        count = count.checked_add(1).ok_or(E::Value)?;
    }
    let count = u32::try_from(count).map_err(|_| E::Num)?;
    let previous = shift(
        maturity,
        i64::from(count)
            .checked_mul(step)
            .and_then(i64::checked_neg)
            .ok_or(E::Value)?,
    )?;
    let next = shift(
        maturity,
        1_i64
            .checked_sub(i64::from(count))
            .and_then(|v| v.checked_mul(step))
            .ok_or(E::Value)?,
    )?;
    let denominator = match basis {
        Basis::Actual => f64::from(next.serial) - f64::from(previous.serial),
        Basis::Actual365 => 365.0 / frequency.payments() as f64,
        _ => 360.0 / frequency.payments() as f64,
    };
    Ok(CouponPeriod {
        previous,
        next,
        count,
        accrued: days(previous, settlement, basis),
        remaining: days(settlement, next, basis),
        denominator,
    })
}
/// Discounted coupon and redemption weights for bond price and duration calculations.
/// Logarithms defer exponentiation until multiplication by money, preserving results whose standalone coefficients underflow or overflow.
struct DiscountFactors {
    coupon_log: f64,
    redemption_log: f64,
    duration: f64,
}
/// Multiply positive cash factors by a discount retained in logarithmic form.
pub(crate) fn discounted_product(factors: &[f64], log_discount: f64) -> Result<f64> {
    finite(factors)?;
    if factors.contains(&0.0) {
        return Ok(0.0);
    }
    let discount = log_discount.exp();
    if discount.is_normal() {
        return scaled_product(factors.iter().copied().chain(std::iter::once(discount)));
    }
    // A subnormal/overflowing coefficient can still yield representable money.
    let logarithm = factors.iter().map(|v| v.ln()).sum::<f64>() + log_discount;
    checked(logarithm.exp())
}
/// Prepared regular-coupon bond used for price, yield, and duration calculations.
/// It retains settlement-period measures and payment terms so repeated yield evaluations share the same calendar.
#[derive(Clone, Copy, Debug)]
pub struct Bond {
    /// Settlement coupon period.
    period: CouponPeriod,
    /// Annual coupon rate.
    coupon: f64,
    /// Final redemption per 100 face value.
    redemption: f64,
    /// Payments per year.
    frequency: Frequency,
}
impl Bond {
    /// Validate the monetary and date inputs of a regular coupon bond.
    pub fn new(
        settlement: Day,
        maturity: Day,
        coupon: f64,
        redemption: f64,
        frequency: Frequency,
        basis: Basis,
    ) -> Result<Self> {
        finite(&[coupon, redemption])?;
        if coupon < 0.0 || redemption <= 0.0 {
            return Err(E::Num);
        }
        Ok(Self {
            period: coupon_period(settlement, maturity, frequency, basis)?,
            coupon,
            redemption,
            frequency,
        })
    }
    fn accrued(self) -> Result<f64> {
        scaled_product([
            100.0,
            self.coupon,
            1.0 / self.frequency.payments() as f64,
            self.period.accrued / self.period.denominator,
        ])
    }
    /// Price per 100 face value, including the documented simple final-period rule.
    pub fn price(self, yield_rate: f64) -> Result<f64> {
        finite(&[yield_rate])?;
        if yield_rate < 0.0 {
            return Err(E::Num);
        }
        self.price_signed(yield_rate)
    }
    fn price_signed(self, yield_rate: f64) -> Result<f64> {
        let f = self.frequency.payments() as f64;
        let t = self.period.remaining / self.period.denominator;
        let accrued = self.period.accrued / self.period.denominator;
        let remaining_accrual =
            (self.period.denominator - self.period.accrued) / self.period.denominator;
        if self.period.count == 1 {
            let r = yield_rate / f;
            let (inverse, coefficient) = if r <= 1.0 {
                let increment = r * t;
                let denominator = 1.0 + increment;
                (
                    1.0 / denominator,
                    (-accrued).mul_add(increment, remaining_accrual) / denominator,
                )
            } else {
                let denominator = 1.0 / r + t;
                let inverse = (1.0 / r) / denominator;
                (
                    inverse,
                    remaining_accrual * inverse - accrued * (t / denominator),
                )
            };
            return checked(
                scaled_product([self.redemption, inverse])?
                    + scaled_product([100.0, self.coupon, 1.0 / f, coefficient])?,
            );
        }
        let factors = self.factors(yield_rate)?;
        let coupon_factor = factors.coupon_log.exp();
        let coupons = if coupon_factor.is_normal() {
            scaled_product([100.0, self.coupon, 1.0 / f, coupon_factor - accrued])?
        } else {
            discounted_product(&[100.0, self.coupon, 1.0 / f], factors.coupon_log)?
                - self.accrued()?
        };
        checked(coupons + discounted_product(&[self.redemption], factors.redemption_log)?)
    }
    fn factors(self, yield_rate: f64) -> Result<DiscountFactors> {
        let f = self.frequency.payments() as f64;
        let r = yield_rate / f;
        if r <= -1.0 {
            return Err(E::Num);
        }
        let t = self.period.remaining / self.period.denominator;
        let last_time = f64::from(self.period.count.checked_sub(1).ok_or(E::Num)?) + t;
        let log = r.ln_1p();
        let redemption_log = -last_time * log;
        if self.coupon == 0.0 {
            return Ok(DiscountFactors {
                coupon_log: f64::NEG_INFINITY,
                redemption_log,
                duration: last_time / f,
            });
        }
        let (sum, weighted) = geometric((-log.abs()).exp(), self.period.count);
        let coupon_time = if r >= 0.0 {
            t + weighted / sum
        } else {
            last_time - weighted / sum
        };
        let coupon_factor_log = sum.ln() - if r >= 0.0 { t * log } else { last_time * log };
        // Cash-flow weights are normalized in log space independently of price underflow.
        let coupon_log = 100.0_f64.ln() + (self.coupon.ln() - f.ln()) + sum.ln()
            - if r >= 0.0 { t * log } else { last_time * log };
        let redemption_weight_log = self.redemption.ln() + redemption_log;
        let center = coupon_log.max(redemption_weight_log);
        let a = (coupon_log - center).exp();
        let b = (redemption_weight_log - center).exp();
        let duration = (a / (a + b) * coupon_time + b / (a + b) * last_time) / f;
        Ok(DiscountFactors {
            coupon_log: coupon_factor_log,
            redemption_log,
            duration: checked(duration)?,
        })
    }
    fn discounted(self, yield_rate: f64) -> Result<(f64, f64)> {
        let factors = self.factors(yield_rate)?;
        let coupons = discounted_product(
            &[100.0, self.coupon, 1.0 / self.frequency.payments() as f64],
            factors.coupon_log,
        )?;
        let redemption = discounted_product(&[self.redemption], factors.redemption_log)?;
        Ok((checked(coupons + redemption)?, factors.duration))
    }
    /// Macaulay duration in years using periodic discounted cash-flow weights.
    pub fn duration(self, yield_rate: f64) -> Result<f64> {
        finite(&[yield_rate])?;
        if yield_rate < 0.0 {
            return Err(E::Num);
        }
        self.factors(yield_rate).map(|factors| factors.duration)
    }
    /// Modified duration with the frequency-adjusted annual yield divisor.
    pub fn modified_duration(self, yield_rate: f64) -> Result<f64> {
        checked(self.duration(yield_rate)? / (1.0 + yield_rate / self.frequency.payments() as f64))
    }
    /// Invert regular pricing with at most 100 charged, residual-certified passes.
    pub fn yield_rate<O>(
        self,
        price: f64,
        mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> std::result::Result<f64, crate::financial::modern::Failure<O>> {
        use crate::financial::modern::Failure::Spreadsheet;
        finite(&[price]).map_err(Spreadsheet)?;
        if price <= 0.0 {
            return Err(Spreadsheet(E::Num));
        }
        let f = self.frequency.payments() as f64;
        let scale = price.max(self.coupon).max(self.redemption);
        let bond = Self {
            coupon: self.coupon / scale,
            redemption: self.redemption / scale,
            ..self
        };
        let price = price / scale;
        let accrued = bond.accrued().map_err(Spreadsheet)?;
        let dirty = price + accrued;
        if self.period.count == 1 {
            let remaining =
                (self.period.denominator - self.period.accrued) / self.period.denominator;
            let numerator = bond.redemption - price + 100.0 * bond.coupon / f * remaining;
            return scaled_quotient(
                [numerator, f, self.period.denominator],
                [dirty, self.period.remaining],
            )
            .map_err(Spreadsheet);
        }
        // Positive cash flows make price strictly decreasing throughout yield > -frequency.
        let mut lo = -f;
        let mut hi = None;
        let mut rate = 0.1_f64;
        let mut prior = f64::INFINITY;
        for _ in 0..100 {
            charge(32).map_err(crate::financial::modern::Failure::Operational)?;
            let (value, duration) = match bond.discounted(rate) {
                Ok(v) => v,
                Err(error) if rate >= 0.0 => return Err(Spreadsheet(error)),
                Err(_) => {
                    lo = rate;
                    rate = (lo + hi.ok_or(Spreadsheet(E::Num))?) * 0.5;
                    continue;
                }
            };
            let residual = value - dirty;
            if residual.abs() <= 2e-12 * dirty && (rate - prior).abs() <= 2e-10 * (1.0 + rate.abs())
            {
                return Ok(rate);
            }
            if residual == 0.0 {
                return Ok(rate);
            }
            if residual > 0.0 {
                lo = rate;
            } else {
                hi = Some(rate);
            }
            let Some(upper) = hi else {
                // Establish a finite upper bracket before Newton refinement.
                prior = rate;
                rate = checked(rate * 2.0 + 1.0).map_err(Spreadsheet)?;
                continue;
            };
            let derivative = -value * duration / (1.0 + rate / f);
            let next = rate - residual / derivative;
            prior = rate;
            rate = if next.is_finite() && next > lo && next < upper {
                next
            } else {
                (lo + upper) * 0.5
            };
            if rate == prior {
                return Err(Spreadsheet(E::Num));
            }
        }
        Err(Spreadsheet(E::Num))
    }
}
// Concatenating geometric blocks gives both sums in logarithmic work without cancellation.
/// Sum a geometric payment block and its zero-based time weights.
pub(crate) fn geometric(q: f64, mut count: u32) -> (f64, f64) {
    let (mut sum, mut weighted, mut power, mut length) = (0.0, 0.0, 1.0, 0.0);
    let (mut block, mut block_weight, mut block_power, mut block_len) = (1.0, 0.0, q, 1.0);
    while count > 0 {
        if count & 1 != 0 {
            weighted += power * (block_weight + length * block);
            sum += power * block;
            power *= block_power;
            length += block_len;
        }
        count >>= 1;
        if count > 0 {
            block_weight += block_power * (block_weight + block_len * block);
            block += block_power * block;
            block_power *= block_power;
            block_len *= 2.0;
        }
    }
    (sum, weighted)
}
/// Accrued interest paid at maturity on a positive principal and rate.
pub fn accrued_at_maturity(
    issue: Day,
    settlement: Day,
    rate: f64,
    par: f64,
    basis: Basis,
) -> Result<f64> {
    finite(&[rate, par])?;
    if issue >= settlement || rate <= 0.0 || par <= 0.0 {
        return Err(E::Num);
    }
    scaled_product([par, rate, fraction(issue, settlement, basis)?])
}
/// Accrued interest over first-interest-anchored quasi periods with charged NASD fragments.
#[allow(clippy::too_many_arguments)]
pub fn accrued_interest<O>(
    issue: Day,
    first: Day,
    settlement: Day,
    rate: f64,
    par: f64,
    frequency: Frequency,
    basis: Basis,
    from_issue: bool,
    mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<f64, crate::financial::modern::Failure<O>> {
    use crate::financial::modern::Failure::{ComputationalLimit, Operational, Spreadsheet};
    if basis != Basis::Nasd {
        return fixed_accrual(
            issue, first, settlement, rate, par, frequency, basis, from_issue,
        )
        .map_err(Spreadsheet);
    }
    finite(&[rate, par]).map_err(Spreadsheet)?;
    if issue >= settlement || first <= issue || rate <= 0.0 || par <= 0.0 {
        return Err(Spreadsheet(E::Num));
    }
    let start = if !from_issue && settlement > first {
        first
    } else {
        issue
    };
    let step = match frequency {
        Frequency::Annual => 12,
        Frequency::Semiannual => 6,
        Frequency::Quarterly => 3,
    };
    let offset = month_index(start.civil())
        .and_then(|a| month_index(first.civil()).and_then(|b| a.checked_sub(b).ok_or(E::Value)))
        .map_err(Spreadsheet)?;
    // SAFETY: step is locally one of 3, 6 or 12, so this division cannot panic.
    let mut index = offset.div_euclid(step);
    if shift(first, index.checked_mul(step).ok_or(Spreadsheet(E::Value))?).map_err(Spreadsheet)?
        > start
    {
        index = index.checked_sub(1).ok_or(Spreadsheet(E::Value))?;
    }
    let mut cursor = start;
    let mut sum = 0.0;
    for _ in 0..100_000 {
        if cursor >= settlement {
            return scaled_quotient([par, rate, sum], [360.0]).map_err(Spreadsheet);
        }
        charge(8).map_err(Operational)?;
        index = index.checked_add(1).ok_or(Spreadsheet(E::Value))?;
        let upper = shift(first, index.checked_mul(step).ok_or(Spreadsheet(E::Value))?)
            .map_err(Spreadsheet)?
            .min(settlement);
        // Integral adjusted day counts sum exactly within this admitted calendar span.
        sum += days(cursor, upper, basis);
        cursor = upper;
    }
    Err(ComputationalLimit)
}
#[allow(clippy::too_many_arguments)]
fn fixed_accrual(
    issue: Day,
    first: Day,
    settlement: Day,
    rate: f64,
    par: f64,
    frequency: Frequency,
    basis: Basis,
    from_issue: bool,
) -> Result<f64> {
    finite(&[rate, par])?;
    if issue >= settlement || first <= issue || rate <= 0.0 || par <= 0.0 {
        return Err(E::Num);
    }
    let start = if !from_issue && settlement > first {
        first
    } else {
        issue
    };
    let f = frequency.payments() as f64;
    let units = if basis != Basis::Actual {
        days(start, settlement, basis) / (year_days(start, settlement, basis)? / f)
    } else {
        let step = match frequency {
            Frequency::Annual => 12,
            Frequency::Semiannual => 6,
            Frequency::Quarterly => 3,
        };
        let position = |date: Day| -> Result<f64> {
            let (a, b) = (date.civil(), first.civil());
            let distance = month_index(a)?
                .checked_sub(month_index(b)?)
                .ok_or(E::Value)?;
            // SAFETY: step is locally one of 3, 6 or 12.
            let mut index = distance.div_euclid(step);
            if shift(first, index.checked_mul(step).ok_or(E::Value)?)? > date {
                index = index.checked_sub(1).ok_or(E::Value)?;
            }
            let lower = shift(first, index.checked_mul(step).ok_or(E::Value)?)?;
            let upper = shift(
                first,
                index
                    .checked_add(1)
                    .and_then(|v| v.checked_mul(step))
                    .ok_or(E::Value)?,
            )?;
            Ok(index as f64
                + (f64::from(date.serial) - f64::from(lower.serial))
                    / (f64::from(upper.serial) - f64::from(lower.serial)))
        };
        position(settlement)? - position(start)?
    };
    scaled_product([par, rate, 1.0 / f, units])
}
/// Direct rate or amount equation for a discounted security.
#[derive(Clone, Copy, Debug)]
pub enum DiscountOperation {
    /// Discount rate relative to redemption.
    Discount,
    /// Investment yield relative to purchase price.
    Yield,
    /// Price from discount and redemption.
    Price,
    /// Redemption from investment and discount.
    Received,
}
/// Evaluate a discounted security with distinct amount denominators.
pub fn discount(
    settlement: Day,
    maturity: Day,
    a: f64,
    b: f64,
    basis: Basis,
    operation: DiscountOperation,
) -> Result<f64> {
    finite(&[a, b])?;
    if settlement >= maturity || a <= 0.0 || b <= 0.0 {
        return Err(E::Num);
    }
    let time = fraction(settlement, maturity, basis)?;
    if time == 0.0 {
        return Err(E::DivZero);
    }
    match operation {
        DiscountOperation::Discount => scaled_quotient([b - a], [b, time]),
        DiscountOperation::Yield => scaled_quotient([b - a], [a, time]),
        DiscountOperation::Price => scaled_product([b, 1.0 - a * time]),
        DiscountOperation::Received => {
            let denominator = 1.0 - b * time;
            if denominator <= 0.0 {
                return Err(E::Num);
            }
            scaled_quotient([a], [denominator])
        }
    }
}
/// Price or yield for a security paying all coupon interest at maturity.
#[derive(Clone, Copy, Debug)]
pub enum MaturityOperation {
    /// Price from annual yield.
    Price,
    /// Annual yield from price.
    Yield,
}
/// Evaluate an interest-at-maturity security with issue-anchored accrued interest.
pub fn maturity_security(
    settlement: Day,
    maturity: Day,
    issue: Day,
    rate: f64,
    value: f64,
    basis: Basis,
    operation: MaturityOperation,
) -> Result<f64> {
    finite(&[rate, value])?;
    if issue > settlement
        || settlement >= maturity
        || rate < 0.0
        || value < 0.0
        || (matches!(operation, MaturityOperation::Yield) && value == 0.0)
    {
        return Err(E::Num);
    }
    let year = year_days(settlement, maturity, basis)?;
    let (a, dim, dsm) = (
        days(issue, settlement, basis) / year,
        days(issue, maturity, basis) / year,
        days(settlement, maturity, basis) / year,
    );
    match operation {
        MaturityOperation::Price => {
            let (inverse, coefficient) = if value < 1.0 {
                let z = value * dsm;
                (1.0 / (1.0 + z), (-a).mul_add(z, dim - a) / (1.0 + z))
            } else {
                let denominator = 1.0 / value + dsm;
                (
                    (1.0 / value) / denominator,
                    ((dim - a) / value - a * dsm) / denominator,
                )
            };
            checked(100.0 * inverse + scaled_product([100.0, rate, coefficient])?)
        }
        MaturityOperation::Yield => {
            let scale = value.max(rate).max(100.0);
            let annual = scaled_product([100.0, rate, 1.0 / scale])?;
            let denominator = value / scale + annual * a;
            if denominator == 0.0 || dsm == 0.0 {
                return Err(E::DivZero);
            }
            checked(
                ((100.0 - value) / scale) / denominator / dsm
                    + scaled_product([
                        100.0,
                        rate,
                        1.0 / scale,
                        dim - a,
                        1.0 / denominator,
                        1.0 / dsm,
                    ])?,
            )
        }
    }
}
/// Treasury bill price, investment yield or bond-equivalent yield.
#[derive(Clone, Copy, Debug)]
pub enum TreasuryOperation {
    /// Price from discount.
    Price,
    /// Annual investment yield from price.
    Yield,
    /// Bond-equivalent yield from discount.
    Equivalent,
}
fn anniversary(day: Day) -> Result<Day> {
    let d = day.civil();
    let year = d.y.checked_add(1).ok_or(E::Value)?;
    Day::from_civil(Date {
        y: year,
        m: d.m,
        d: d.d.min(month_days(year, d.m)),
    })
}
/// Evaluate a Treasury bill over at most one calendar year using actual elapsed days.
pub fn treasury(
    settlement: Day,
    maturity: Day,
    value: f64,
    operation: TreasuryOperation,
) -> Result<f64> {
    finite(&[value])?;
    if value <= 0.0
        || settlement > maturity
        || maturity > anniversary(settlement)?
        || (matches!(operation, TreasuryOperation::Yield) && settlement == maturity)
    {
        return Err(E::Num);
    }
    let days = f64::from(maturity.serial) - f64::from(settlement.serial);
    match operation {
        TreasuryOperation::Price => checked(100.0 * (1.0 - value * (days / 360.0))),
        TreasuryOperation::Yield => scaled_quotient([100.0 - value, 360.0], [value, days]),
        TreasuryOperation::Equivalent => {
            let product = value * days;
            if product.is_finite() {
                let denominator = 360.0 - product;
                if denominator == 0.0 {
                    return Err(E::DivZero);
                }
                scaled_quotient([value, 365.0], [denominator])
            } else {
                checked(365.0 / (360.0 / value - days))
            }
        }
    }
}

/// Measure an anchored quasi-coupon interval, charging nonadditive day-count fragments.
pub(crate) fn quasi_fraction<O>(
    start: Day,
    end: Day,
    anchor: Day,
    frequency: Frequency,
    basis: Basis,
    mut charge: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<f64, crate::financial::modern::Failure<O>> {
    use crate::financial::modern::Failure::{ComputationalLimit, Operational, Spreadsheet};
    if start > end {
        return Err(Spreadsheet(E::Num));
    }
    let step = 12_i64
        .checked_div(frequency.payments())
        .ok_or(Spreadsheet(E::Num))?;
    let position = |day: Day| -> Result<(i64, Day, Day)> {
        let distance = month_index(day.civil())?
            .checked_sub(month_index(anchor.civil())?)
            .ok_or(E::Value)?;
        let mut index = distance.checked_div_euclid(step).ok_or(E::Value)?;
        if shift(anchor, index.checked_mul(step).ok_or(E::Value)?)? > day {
            index = index.checked_sub(1).ok_or(E::Value)?;
        }
        Ok((
            index,
            shift(anchor, index.checked_mul(step).ok_or(E::Value)?)?,
            shift(
                anchor,
                index
                    .checked_add(1)
                    .and_then(|v| v.checked_mul(step))
                    .ok_or(E::Value)?,
            )?,
        ))
    };
    let (mut index, lower, upper) = position(start).map_err(Spreadsheet)?;
    if basis == Basis::Actual {
        let (last, lo, hi) = position(end).map_err(Spreadsheet)?;
        let a = days(lower, start, basis) / days(lower, upper, basis);
        let b = days(lo, end, basis) / days(lo, hi, basis);
        return checked(last.checked_sub(index).ok_or(Spreadsheet(E::Value))? as f64 + b - a)
            .map_err(Spreadsheet);
    }
    let denominator = if basis == Basis::Actual365 {
        365.0
    } else {
        360.0
    } / frequency.payments() as f64;
    if matches!(basis, Basis::Actual360 | Basis::Actual365) {
        return checked(days(start, end, basis) / denominator).map_err(Spreadsheet);
    }
    let mut cursor = start;
    let mut sum = 0.0;
    for _ in 0..100_000 {
        if cursor >= end {
            return checked(sum / denominator).map_err(Spreadsheet);
        }
        charge(8).map_err(Operational)?;
        index = index.checked_add(1).ok_or(Spreadsheet(E::Value))?;
        let upper = shift(
            anchor,
            index.checked_mul(step).ok_or(Spreadsheet(E::Value))?,
        )
        .map_err(Spreadsheet)?
        .min(end);
        sum += days(cursor, upper, basis);
        cursor = upper;
    }
    Err(ComputationalLimit)
}

/// Measure the depreciation consumer's symmetric month adjustment and leap-containing year.
pub(crate) fn depreciation_fraction(start: Day, end: Day, basis: Basis) -> Result<f64> {
    let (a, b) = (start.civil(), end.civil());
    if matches!(basis, Basis::Nasd | Basis::European) {
        let adjust = |d: Date| {
            if d.d == 31 || (d.m == 2 && d.d >= 28) {
                30
            } else {
                d.d
            }
        };
        return checked(
            ((b.y as f64 - a.y as f64) * 360.0
                + (b.m as f64 - a.m as f64) * 30.0
                + (adjust(b) as f64 - adjust(a) as f64))
                / 360.0,
        );
    }
    let mut leap = false;
    if basis == Basis::Actual {
        if b.y as f64 - a.y as f64 > 8.0 {
            leap = true;
        } else {
            // Gregorian century omissions permit at most eight years between leap days.
            for offset in 0..=8 {
                let year = a.y.checked_add(offset).ok_or(E::Value)?;
                if year > b.y {
                    break;
                }
                if calendar::leap(year) || year == 1900 {
                    let feb = Day::from_civil(Date {
                        y: year,
                        m: 2,
                        d: 29,
                    })?;
                    leap |= start <= feb && feb <= end;
                }
            }
        }
    }
    checked(days(start, end, Basis::Actual) / if leap { 366.0 } else { 365.0 })
}
