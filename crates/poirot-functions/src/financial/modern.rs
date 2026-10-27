//! Independent cash-flow and depreciation mathematics from the documented equations.
//! Contract links and deliberately unresolved native details are in FINANCIAL_SOURCES.md.
/// Convert a positive nominal annual rate to its effective annual rate.
pub fn effect(rate: f64, periods: f64) -> Result<f64> {
    finite(&[rate, periods])?;
    let n = periods.trunc();
    if rate <= 0.0 || n < 1.0 {
        return Err(E::Num);
    }
    let part = rate / n;
    checked(if part == 0.0 {
        rate.exp_m1()
    } else {
        (n * part.ln_1p()).exp_m1()
    })
}
/// Convert a positive effective annual rate to a nominal annual rate.
pub fn nominal(rate: f64, periods: f64) -> Result<f64> {
    finite(&[rate, periods])?;
    let n = periods.trunc();
    if rate <= 0.0 || n < 1.0 {
        return Err(E::Num);
    }
    let exponent = rate.ln_1p() / n;
    checked(if exponent == 0.0 {
        rate.ln_1p()
    } else {
        n * exponent.exp_m1()
    })
}
/// Calculate the periods needed for positive compound growth.
pub fn pduration(rate: f64, present: f64, future: f64) -> Result<f64> {
    finite(&[rate, present, future])?;
    if rate <= 0.0 || present <= 0.0 || future <= 0.0 {
        return Err(E::Num);
    }
    checked(log_ratio(future, present) / rate.ln_1p())
}
/// Calculate the real positive-base equivalent growth rate.
pub fn rri(periods: f64, present: f64, future: f64) -> Result<f64> {
    finite(&[periods, present, future])?;
    if periods <= 0.0
        || present == 0.0
        || (future != 0.0 && present.is_sign_negative() != future.is_sign_negative())
    {
        return Err(E::Num);
    }
    if future == 0.0 {
        return Ok(-1.0);
    }
    checked((log_ratio(future.abs(), present.abs()) / periods).exp_m1())
}
/// Calculate interest for a zero-based even-principal loan period.
pub fn ispmt(rate: f64, period: f64, periods: f64, present: f64) -> Result<f64> {
    finite(&[rate, period, periods, present])?;
    if periods == 0.0 {
        return Err(E::DivZero);
    }
    checked((period / periods - 1.0) * present * rate)
}
/// Calculate straight-line depreciation for one period.
pub fn sln(cost: f64, salvage: f64, life: f64) -> Result<f64> {
    finite(&[cost, salvage, life])?;
    if life == 0.0 {
        return Err(E::DivZero);
    }
    let difference = cost - salvage;
    checked(if difference.is_finite() {
        difference / life
    } else {
        cost / life - salvage / life
    })
}
/// Calculate sum-of-years-digits depreciation for the selected period.
pub fn syd(cost: f64, salvage: f64, life: f64, period: f64) -> Result<f64> {
    finite(&[cost, salvage, life, period])?;
    if life <= 0.0 || period <= 0.0 || period > life {
        return Err(E::Num);
    }
    checked(sln(cost, salvage, life)? * (2.0 * ((life - period + 1.0) / (life + 1.0))))
}
/// Select the loan component without confusing principal and interest totals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoanComponent {
    /// Sum the principal repaid by the selected payments.
    Principal,
    /// Sum the interest charged by the selected payments.
    Interest,
}
// log((1-exp(-x))/x), retaining the small correction when exp(-x) rounds to one.
fn log_discount_mean(x: f64) -> f64 {
    if x.abs() < 1e-3 {
        -x / 2.0 + x * x / 24.0 - x.powi(4) / 2880.0
    } else {
        (-(-x).exp_m1() / x).ln()
    }
}
/// Sum inclusive loan payments using geometric endpoints, without a period walk.
pub fn cumulative(
    component: LoanComponent,
    rate: f64,
    periods: f64,
    present: f64,
    start: f64,
    end: f64,
    timing: f64,
) -> Result<f64> {
    finite(&[rate, periods, present, start, end, timing])?;
    if rate <= 0.0
        || periods <= 0.0
        || present <= 0.0
        || start < 1.0
        || end < start
        || end > periods
        || (timing != 0.0 && timing != 1.0)
    {
        return Err(E::Num);
    }
    let start = start.trunc();
    let end = end.trunc();
    let l = rate.ln_1p();
    let denominator = -(-periods * l).exp_m1();
    let payment = present * ((rate / (1.0 + rate).powf(timing)) / denominator);
    let first = if timing == 1.0 && start == 1.0 {
        payment
    } else {
        0.0
    };
    let lower = if timing == 1.0 { start.max(2.0) } else { start };
    let count = if end < lower {
        0.0
    } else {
        (end - lower) + 1.0
    };
    let principal = if count == 0.0 {
        0.0
    } else {
        present * (((end - periods) - timing) * l).exp() * (-(-count * l).exp_m1() / denominator)
    } + first;
    if component == LoanComponent::Principal {
        return checked(-principal);
    }
    if count == 0.0 {
        return Ok(0.0);
    }
    // The log(log1p(r)/r) series through degree six has its first omitted
    // term of order r^7; the 1e-3 cutoff keeps truncation below rounding noise.
    let rate_remainder = if rate < 1e-3 {
        rate * rate
            * (5.0 / 24.0
                + rate
                    * (-0.125
                        + rate
                            * (251.0 / 2880.0
                                + rate * (-19.0 / 288.0 + rate * (19087.0 / 362880.0)))))
    } else {
        0.0
    };
    let x = count * l;
    let corrections = if rate < 1e-3 && x < 1e-3 {
        // Combine the two linear terms before halving: each separate half can
        // underflow even when their sum affects a representable interest total.
        -(x + rate) / 2.0 + x * x / 24.0 - x.powi(4) / 2880.0 + rate_remainder
    } else {
        log_discount_mean(x)
            + if rate < 1e-3 {
                -rate / 2.0 + rate_remainder
            } else {
                (l / rate).ln()
            }
    };
    // The non-initial beginning payments match end-payment principal amounts,
    // but their total payment is smaller by q. Express their ratio in log space.
    let log_ratio = (end - periods) * l + corrections;
    scaled_product([
        present,
        (rate / (1.0 + rate).powf(timing)) / denominator,
        count,
        log_ratio.exp_m1(),
    ])
}
/// Calculate fixed declining-balance depreciation with rounded rate and partial years.
pub fn db(cost: f64, salvage: f64, life: f64, period: f64, month: f64) -> Result<f64> {
    finite(&[cost, salvage, life, period, month])?;
    let p = period.trunc();
    let m = month.trunc();
    if cost < 0.0
        || salvage < 0.0
        || life <= 0.0
        || p < 1.0
        || p > life.ceil() + f64::from(m != 12.0)
        || !(1.0..=12.0).contains(&m)
    {
        return Err(E::Num);
    }
    if cost == 0.0 || salvage >= cost {
        return Ok(0.0);
    }
    let raw = if salvage == 0.0 {
        1.0
    } else {
        -(log_ratio(salvage, cost) / life).exp_m1()
    };
    let rate = (raw * 1000.0).round() / 1000.0;
    if p == 1.0 {
        return checked(cost * rate * (m / 12.0));
    }
    let balance = cost * (1.0 - rate * m / 12.0) * (1.0 - rate).powf(p - 2.0);
    checked(balance * rate * if p > life { (12.0 - m) / 12.0 } else { 1.0 })
}
/// Calculate one capped declining-balance period without a straight-line switch.
pub fn ddb(cost: f64, salvage: f64, life: f64, period: f64, factor: f64) -> Result<f64> {
    finite(&[cost, salvage, life, period, factor])?;
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || period <= 0.0 || period > life || factor <= 0.0
    {
        return Err(E::Num);
    }
    if cost <= salvage {
        return Ok(0.0);
    }
    let rate = (factor / life).min(1.0);
    let balance = if period <= 1.0 {
        cost
    } else {
        cost * ((period - 1.0) * (-rate).ln_1p()).exp()
    };
    checked((balance * rate).min((balance - salvage).max(0.0)))
}

use super::financial_calculation_error::FinancialCalculationError;
use crate::financial::{FinancialError as E, Result, checked, finite};

fn log_ratio(a: f64, b: f64) -> f64 {
    let relative = (a - b) / b;
    if relative.abs() < 0.5 {
        relative.ln_1p()
    } else {
        a.ln() - b.ln()
    }
}

/// Asset amounts and useful life for variable declining-balance depreciation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Asset {
    /// Initial nonnegative asset cost.
    pub cost: f64,
    /// Nonnegative value remaining after depreciation.
    pub salvage: f64,
    /// Positive useful life in the same units as the requested interval.
    pub life: f64,
}
/// Calculate fractional-period variable depreciation with a metered straight-line switch.
pub fn vdb<F>(
    asset: Asset,
    start: f64,
    end: f64,
    factor: f64,
    no_switch: bool,
    mut charge: impl FnMut(u64) -> std::result::Result<(), F>,
) -> std::result::Result<f64, FinancialCalculationError<F>> {
    let Asset {
        cost,
        salvage,
        life,
    } = asset;
    finite(&[cost, salvage, life, start, end, factor])
        .map_err(FinancialCalculationError::Spreadsheet)?;
    if cost < 0.0
        || salvage < 0.0
        || life <= 0.0
        || start < 0.0
        || end < start
        || end > life
        || factor <= 0.0
    {
        return Err(FinancialCalculationError::Spreadsheet(E::Num));
    }
    if cost <= salvage || start == end {
        return Ok(0.0);
    }
    if end.ceil() > 100_000.0 {
        return Err(FinancialCalculationError::ComputationalLimit);
    }
    let rate = (factor / life).min(1.0);
    let mut balance = cost;
    let mut total = 0.0;
    let mut period = 0.0;
    while period < end {
        charge(1).map_err(FinancialCalculationError::Operational)?;
        let remaining = (balance - salvage).max(0.0);
        let declining = (balance * rate).min(remaining);
        let remaining_time = life - period;
        let straight_line = !no_switch && remaining > declining * remaining_time;
        let width = (end.min(period + 1.0) - start.max(period)).max(0.0);
        let period_width = remaining_time.min(1.0);
        let (contribution, period_depreciation) = if straight_line {
            (
                remaining * (width / remaining_time),
                remaining * (period_width / remaining_time),
            )
        } else {
            (declining * width, declining * period_width)
        };
        total += contribution;
        balance = (balance - period_depreciation).max(salvage);
        period += 1.0;
    }
    checked(total).map_err(FinancialCalculationError::Spreadsheet)
}
/// Multiply a complete schedule while preserving recoverable intermediate exponents.
pub fn fvschedule(principal: f64, schedule: &[f64]) -> Result<f64> {
    finite(&[principal])?;
    finite(schedule)?;
    scaled_product(std::iter::once(principal).chain(schedule.iter().map(|r| 1.0 + r)))
}
// Binary normalization keeps intermediate exponents separate without allocating.
/// Multiply factors while retaining recoverable binary exponents.
pub(crate) fn scaled_product(factors: impl IntoIterator<Item = f64>) -> Result<f64> {
    scaled_quotient(factors, std::iter::empty())
}
/// Multiply and divide finite factors without materializing overflowing reciprocals.
pub(crate) fn scaled_quotient(
    numerators: impl IntoIterator<Item = f64>,
    denominators: impl IntoIterator<Item = f64>,
) -> Result<f64> {
    let mut mantissa = 1.0_f64;
    let mut exponent = 0.0;
    let factors = numerators
        .into_iter()
        .map(|v| (v, false))
        .chain(denominators.into_iter().map(|v| (v, true)));
    for (factor, divide) in factors {
        finite(&[factor])?;
        if divide && factor == 0.0 {
            return Err(E::DivZero);
        }
        if factor == 0.0 || mantissa == 0.0 {
            mantissa = 0.0;
            continue;
        }
        let e = factor.abs().log2().floor().clamp(-1022.0, 1023.0);
        let f = factor * 2.0_f64.powf(-e);
        let m = mantissa.abs().log2().floor().clamp(-1022.0, 1023.0);
        let normalized = mantissa * 2.0_f64.powf(-m);
        mantissa = if divide {
            normalized / f
        } else {
            normalized * f
        };
        exponent += m + if divide { -e } else { e };
    }
    if mantissa == 0.0 {
        return Ok(mantissa);
    }
    let e = exponent.clamp(-1022.0, 1023.0);
    checked((mantissa * 2.0_f64.powf(exponent - e)) * 2.0_f64.powf(e))
}

/// A cash flow paired with its day offset from the original first date for XNPV and XIRR.
/// Offsets preserve source order and are checked for finiteness and nonnegativity by the consuming algorithms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DatedFlow {
    /// Cash received or paid at this position.
    pub value: f64,
    /// Nonnegative serial-day difference from the first position.
    pub days: f64,
}
/// Compensated addition that retains small terms between opposite large terms.
#[derive(Default)]
struct Sum {
    value: f64,
    correction: f64,
}
impl Sum {
    fn add(&mut self, term: f64) {
        let next = self.value + term;
        self.correction += if self.value.abs() >= term.abs() {
            (self.value - next) + term
        } else {
            (term - next) + self.value
        };
        self.value = next;
    }
    fn total(&self) -> f64 {
        self.value + self.correction
    }
}
/// Reconstruction scale for normalized discounted values.
enum DiscountScale {
    /// An ordinary finite scale keeps zero-rate sums in linear arithmetic.
    Linear(f64),
    /// Exponential discounts may require a scale outside binary64 magnitude.
    Logarithmic(f64),
}
/// Scaled cash-flow equation and derivative used by XNPV and the XIRR solver.
/// The absolute-term sum tests convergence relative to the discounted cash-flow magnitude, while the scale reconstructs monetary results.
struct Discounted {
    value: f64,
    derivative: f64,
    mass: f64,
    scale: DiscountScale,
}
fn discounted(flows: &[DatedFlow], log_rate: f64) -> Result<Discounted> {
    if log_rate == 0.0 {
        let scale = flows.iter().map(|f| f.value.abs()).fold(0.0_f64, f64::max);
        let (mut sum, mut derivative, mut mass) = (Sum::default(), Sum::default(), 0.0);
        if scale != 0.0 {
            for f in flows {
                let term = f.value / scale;
                sum.add(term);
                derivative.add(-f.days / 365.0 * term);
                mass += term.abs();
            }
        }
        return Ok(Discounted {
            value: sum.total(),
            derivative: derivative.total(),
            mass,
            scale: DiscountScale::Linear(scale),
        });
    }
    let scale = flows
        .iter()
        .filter(|f| f.value != 0.0)
        .map(|f| f.value.abs().ln() - f.days / 365.0 * log_rate)
        .fold(f64::NEG_INFINITY, f64::max);
    if scale == f64::NEG_INFINITY {
        return Ok(Discounted {
            value: 0.0,
            derivative: 0.0,
            mass: 0.0,
            scale: DiscountScale::Linear(0.0),
        });
    }
    if !scale.is_finite() {
        return Err(E::Num);
    }
    let (mut sum, mut derivative, mut mass) = (Sum::default(), Sum::default(), 0.0);
    for f in flows {
        if f.value == 0.0 {
            continue;
        }
        let t = f.days / 365.0;
        let term = (f.value.abs().ln() - t * log_rate - scale)
            .exp()
            .copysign(f.value);
        sum.add(term);
        derivative.add(-t * term);
        mass += term.abs();
    }
    Ok(Discounted {
        value: sum.total(),
        derivative: derivative.total(),
        mass,
        scale: DiscountScale::Logarithmic(scale),
    })
}
fn validate_flows(flows: &[DatedFlow]) -> Result<()> {
    if flows.is_empty()
        || flows
            .iter()
            .any(|f| !f.value.is_finite() || !f.days.is_finite() || f.days < 0.0)
    {
        return Err(E::Num);
    }
    Ok(())
}
/// Discount irregular cash flows on a 365-day year from their original first date.
pub fn xnpv(rate: f64, flows: &[DatedFlow]) -> Result<f64> {
    validate_flows(flows)?;
    if !rate.is_finite() || rate <= -1.0 {
        return Err(E::Num);
    }
    let Discounted {
        value: sum, scale, ..
    } = discounted(flows, rate.ln_1p())?;
    if sum == 0.0 {
        return Ok(0.0);
    }
    match scale {
        DiscountScale::Linear(scale) => scaled_product([sum, scale]),
        DiscountScale::Logarithmic(scale) => checked((sum.abs().ln() + scale).exp().copysign(sum)),
    }
}
/// Find an irregular-flow root from a guess with at most 100 metered Newton passes.
pub fn xirr<F>(
    flows: &[DatedFlow],
    guess: f64,
    mut charge: impl FnMut(u64) -> std::result::Result<(), F>,
) -> std::result::Result<f64, FinancialCalculationError<F>> {
    let count = flows.len() as u64;
    charge(
        count
            .checked_mul(4)
            .ok_or(FinancialCalculationError::ComputationalLimit)?,
    )
    .map_err(FinancialCalculationError::Operational)?;
    validate_flows(flows).map_err(FinancialCalculationError::Spreadsheet)?;
    if !guess.is_finite()
        || guess <= -1.0
        || !flows.iter().any(|f| f.value > 0.0)
        || !flows.iter().any(|f| f.value < 0.0)
        || !flows.iter().any(|f| f.days > 0.0)
    {
        return Err(FinancialCalculationError::Spreadsheet(E::Num));
    }
    let mut rate = guess;
    for _ in 0..100 {
        charge(
            count
                .checked_mul(2)
                .ok_or(FinancialCalculationError::ComputationalLimit)?,
        )
        .map_err(FinancialCalculationError::Operational)?;
        let Discounted {
            value,
            derivative,
            mass,
            ..
        } = discounted(flows, rate.ln_1p()).map_err(FinancialCalculationError::Spreadsheet)?;
        if value == 0.0 {
            return Ok(rate);
        }
        if derivative == 0.0 || !derivative.is_finite() {
            return Err(FinancialCalculationError::Spreadsheet(E::Num));
        }
        let step = value / derivative * (1.0 + rate);
        if value.abs() <= mass * 1e-14 && step.abs() <= 1e-13 * (1.0 + rate.abs()) {
            return Ok(rate);
        }
        let next = rate - step;
        if !next.is_finite() || next <= -1.0 || next == rate {
            return Err(FinancialCalculationError::Spreadsheet(E::Num));
        }
        rate = next;
    }
    Err(FinancialCalculationError::Spreadsheet(E::Num))
}
