//! Modern distribution equations with separate domain, convergence and work failures.
use super::modern_tails::{self as tails, BetaCoordinate, Tail};
use poirot_functions_core::CellError;
/// Failure retaining the distinction between mathematical errors and execution limits.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure<O> {
    /// Invalid worksheet domain or nonrepresentable mathematical result.
    Spreadsheet(CellError),
    /// Caller work or execution failure.
    Operational(O),
    /// A finite numerical iteration did not meet its convergence criterion.
    NonConvergence,
    /// Requested work exceeds the numerical implementation's finite allowance.
    ComputationalLimit,
}
/// Numerical value or a typed mathematical/operational failure.
pub type Result<O> = std::result::Result<f64, Failure<O>>;
fn num<O>() -> Failure<O> {
    Failure::Spreadsheet(CellError::Num)
}
fn finite<O>(x: f64) -> Result<O> {
    if x.is_finite() { Ok(x) } else { Err(num()) }
}
fn inputs<O>(values: &[f64]) -> std::result::Result<(), Failure<O>> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(num())
    }
}
fn count<O>(x: f64) -> Result<O> {
    if x.is_finite() && x.trunc() >= 0.0 {
        Ok(x.trunc())
    } else {
        Err(num())
    }
}
fn bounded_count<O>(x: f64) -> Result<O> {
    let n = count(x)?;
    if n > 1e12 {
        Err(Failure::ComputationalLimit)
    } else {
        Ok(n)
    }
}
/// Standard normal density, evaluated directly in the log domain.
pub fn phi<O>(x: f64) -> Result<O> {
    inputs(&[x])?;
    Ok((-0.5 * x * x - 0.9189385332046727).exp())
}
/// Signed standard normal integral from zero, retaining small arguments.
pub fn gauss<O>(x: f64) -> Result<O> {
    inputs(&[x])?;
    Ok(0.5 * libm::erf(x * std::f64::consts::FRAC_1_SQRT_2))
}
/// Euler gamma with signed negative nonintegral arguments.
pub fn gamma_value<O>(x: f64) -> Result<O> {
    inputs(&[x])?;
    if x == 0.0 || x < 0.0 && x == x.trunc() {
        return Err(num());
    }
    finite(libm::tgamma(x))
}
/// Logarithm of gamma for a positive argument, without forming gamma first.
pub fn gamma_log<O>(x: f64) -> Result<O> {
    inputs(&[x])?;
    if x <= 0.0 {
        return Err(num());
    }
    finite(libm::lgamma(x))
}
/// Exponential density or cumulative probability with a rate parameter.
pub fn exponential<O>(x: f64, rate: f64, cumulative: bool) -> Result<O> {
    inputs(&[x, rate])?;
    if x < 0.0 || rate <= 0.0 {
        return Err(num());
    }
    let t = x * rate;
    if cumulative {
        Ok(-(-t).exp_m1())
    } else {
        finite((rate.ln() - t).exp())
    }
}
/// Weibull density or cumulative probability with shape and scale parameters.
pub fn weibull<O>(x: f64, shape: f64, scale: f64, cumulative: bool) -> Result<O> {
    inputs(&[x, shape, scale])?;
    if x < 0.0 || shape <= 0.0 || scale <= 0.0 {
        return Err(num());
    }
    if x == 0.0 {
        return if cumulative || shape > 1.0 {
            Ok(0.0)
        } else if shape == 1.0 {
            finite(1.0 / scale)
        } else {
            Err(num())
        };
    }
    let log_ratio = ratio_log(x, scale);
    let log_t = shape * log_ratio;
    if cumulative {
        Ok(-(-log_t.exp()).exp_m1())
    } else if log_t > 710.0 {
        Ok(0.0)
    } else {
        finite((shape.ln() - scale.ln() + (shape - 1.0) * log_ratio - log_t.exp()).exp())
    }
}
/// Lognormal density or cumulative probability, avoiding an overflowing x*sigma.
pub fn lognormal<O>(x: f64, mean: f64, sigma: f64, cumulative: bool) -> Result<O> {
    inputs(&[x, mean, sigma])?;
    if x <= 0.0 || sigma <= 0.0 {
        return Err(num());
    }
    let z = (x.ln() - mean) / sigma;
    if cumulative {
        Ok(0.5 * libm::erfc(-z * std::f64::consts::FRAC_1_SQRT_2))
    } else {
        finite((-0.5 * z * z - x.ln() - sigma.ln() - 0.9189385332046727).exp())
    }
}
/// Log ratio preserving close finite operands and extreme positive scales.
fn ratio_log(x: f64, y: f64) -> f64 {
    let difference = (x - y) / y;
    if difference.abs() <= 0.5 {
        return difference.ln_1p();
    }
    let ratio = x / y;
    if ratio.is_normal() {
        ratio.ln()
    } else {
        x.ln() - y.ln()
    }
}
/// Gamma density or cumulative probability; beta is a scale, not a rate.
pub fn gamma_dist<O>(
    x: f64,
    shape: f64,
    scale: f64,
    cumulative: bool,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[x, shape, scale])?;
    if x < 0.0 || shape <= 0.0 || scale <= 0.0 {
        return Err(num());
    }
    if shape > 1e12 {
        return Err(Failure::ComputationalLimit);
    }
    if x == 0.0 {
        return if cumulative || shape > 1.0 {
            Ok(0.0)
        } else if shape == 1.0 {
            finite(1.0 / scale)
        } else {
            Err(num())
        };
    }
    let log_ratio = ratio_log(x, scale);
    let ratio = x / scale;
    if cumulative {
        if log_ratio < -230.0 {
            meter(1).map_err(Failure::Operational)?;
            return Ok((shape * log_ratio - libm::lgamma(shape + 1.0))
                .exp()
                .min(1.0));
        }
        tails::gamma(shape, ratio, Tail::Lower, meter)
    } else {
        let log = if shape >= 128.0 && ratio.is_finite() && ((ratio - shape) / shape).abs() < 0.125
        {
            // Density in standardized coordinates divided by sqrt(shape)*scale.
            tails::gamma_log_density(shape, ratio) - scale.ln()
        } else {
            (shape - 1.0) * log_ratio - ratio - libm::lgamma(shape) - scale.ln()
        };
        finite(log.exp())
    }
}
/// Chi-square density or selected probability tail, truncating the degrees.
pub fn chi_square<O>(
    x: f64,
    degrees: f64,
    kind: DistributionKind,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[x, degrees])?;
    let n = degrees.trunc();
    if x < 0.0 || !(1.0..=1e10).contains(&n) {
        return Err(num());
    }
    match kind {
        DistributionKind::Density => gamma_dist(x, n * 0.5, 2.0, false, meter),
        DistributionKind::Lower => gamma_dist(x, n * 0.5, 2.0, true, meter),
        DistributionKind::Upper => tails::gamma(n * 0.5, x * 0.5, Tail::Upper, meter),
    }
}
/// Requested density or probability side for continuous distributions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistributionKind {
    /// Probability density.
    Density,
    /// Cumulative lower-tail probability.
    Lower,
    /// Upper-tail probability.
    Upper,
}
/// Beta density or CDF on an arbitrary finite interval.
pub fn beta_dist<O>(
    x: f64,
    a: f64,
    b: f64,
    cumulative: bool,
    lower: f64,
    upper: f64,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[x, a, b, lower, upper])?;
    if a <= 0.0 || b <= 0.0 || lower >= upper || x < lower || x > upper {
        return Err(num());
    }
    if a > 1e12 || b > 1e12 {
        return Err(Failure::ComputationalLimit);
    }
    // Half-scale differences also support bounds straddling ±MAX without overflow.
    let width = upper - lower;
    let log_width = if width.is_finite() {
        width.ln()
    } else {
        (upper * 0.5 - lower * 0.5).ln() + std::f64::consts::LN_2
    };
    let y = if width.is_finite() {
        (x - lower) / width
    } else {
        (x * 0.5 - lower * 0.5) / (upper * 0.5 - lower * 0.5)
    };
    let z = if width.is_finite() {
        (upper - x) / width
    } else {
        (upper * 0.5 - x * 0.5) / (upper * 0.5 - lower * 0.5)
    };
    let log_y = if y > 0.0 {
        y.ln()
    } else if x == lower {
        f64::NEG_INFINITY
    } else {
        (x - lower).ln() - log_width
    };
    let log_z = if z > 0.0 {
        z.ln()
    } else if x == upper {
        f64::NEG_INFINITY
    } else {
        (upper - x).ln() - log_width
    };
    if cumulative {
        let coordinate = if y <= z {
            if y.is_normal() {
                BetaCoordinate::Lower(y)
            } else {
                BetaCoordinate::LowerLog(log_y)
            }
        } else if z.is_normal() {
            BetaCoordinate::Upper(z)
        } else {
            BetaCoordinate::UpperLog(log_z)
        };
        tails::beta(a, b, coordinate, Tail::Lower, meter)
    } else {
        if x == lower {
            return if a < 1.0 {
                Err(num())
            } else if a > 1.0 {
                Ok(0.0)
            } else {
                finite((b.ln() - log_width).exp())
            };
        }
        if x == upper {
            return if b < 1.0 {
                Err(num())
            } else if b > 1.0 {
                Ok(0.0)
            } else {
                finite((a.ln() - log_width).exp())
            };
        }
        finite((tails::beta_log_weight_logs(a, b, log_y, log_z) - log_y - log_z - log_width).exp())
    }
}
/// Fisher F density or tail using a stable ratio and its retained small complement.
pub fn fisher_f<O>(
    x: f64,
    d1: f64,
    d2: f64,
    kind: DistributionKind,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[x, d1, d2])?;
    let d1 = d1.trunc();
    let d2 = d2.trunc();
    if x < 0.0 || d1 < 1.0 || d2 < 1.0 {
        return Err(num());
    }
    if d1 > 2e12 || d2 > 2e12 {
        return Err(Failure::ComputationalLimit);
    }
    if x == 0.0 {
        return match kind {
            DistributionKind::Lower => Ok(0.0),
            DistributionKind::Upper => Ok(1.0),
            DistributionKind::Density => {
                if d1 < 2.0 {
                    Err(num())
                } else if d1 == 2.0 {
                    Ok(1.0)
                } else {
                    Ok(0.0)
                }
            }
        };
    }
    let log_r = x.ln() + ratio_log(d1, d2);
    let ratio = x * (d1 / d2);
    let coordinate = if ratio.is_normal() && ratio.is_finite() {
        if ratio <= 1.0 {
            BetaCoordinate::Lower(ratio / (1.0 + ratio))
        } else {
            BetaCoordinate::Upper(1.0 / (1.0 + ratio))
        }
    } else if log_r < 0.0 {
        BetaCoordinate::LowerLog(log_r - log_r.exp().ln_1p())
    } else {
        BetaCoordinate::UpperLog(-log_r - (-log_r).exp().ln_1p())
    };
    let (log_y, log_z) = if log_r >= 0.0 {
        (-(-log_r).exp().ln_1p(), -log_r - (-log_r).exp().ln_1p())
    } else {
        (log_r - log_r.exp().ln_1p(), -log_r.exp().ln_1p())
    };
    let a = d1 * 0.5;
    let b = d2 * 0.5;
    match kind {
        DistributionKind::Density => {
            finite((tails::beta_log_weight_logs(a, b, log_y, log_z) - x.ln()).exp())
        }
        _ => tails::beta(
            a,
            b,
            coordinate,
            if kind == DistributionKind::Lower {
                Tail::Lower
            } else {
                Tail::Upper
            },
            meter,
        ),
    }
}
/// Binomial point probability using a cancellation-resistant beta normalization.
fn binomial_log_mass(n: f64, k: f64, p: f64) -> f64 {
    if p == 0.0 {
        return if k == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    if p == 1.0 {
        return if k == n { 0.0 } else { f64::NEG_INFINITY };
    }
    if k == 0.0 {
        return n * (-p).ln_1p();
    }
    if k == n {
        return n * p.ln();
    }
    tails::beta_log_weight(k, n - k, p, 1.0 - p) + n.ln() - k.ln() - (n - k).ln()
}
/// Binomial mass or CDF with integer-truncated successes and trial count.
pub fn binomial<O>(
    k: f64,
    n: f64,
    p: f64,
    cumulative: bool,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[k, n, p])?;
    let n = bounded_count(n)?;
    let k = count(k)?;
    if k > n || !(0.0..=1.0).contains(&p) {
        return Err(num());
    }
    if !cumulative {
        return Ok(binomial_log_mass(n, k, p).exp());
    }
    if k == n || p == 0.0 {
        return Ok(1.0);
    }
    if p == 1.0 {
        return Ok(0.0);
    }
    tails::beta(n - k, k + 1.0, BetaCoordinate::Upper(p), Tail::Lower, meter)
}
/// Inclusive binomial interval with bounded local summation or favorable tail differences.
pub fn binomial_range<O>(
    n: f64,
    p: f64,
    low: f64,
    high: f64,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[n, p, low, high])?;
    let n = bounded_count(n)?;
    let low = count(low)?;
    let high = count(high)?;
    if low > high || high > n || !(0.0..=1.0).contains(&p) {
        return Err(num());
    }
    if high - low <= 4096.0 {
        let mut sum = 0.0;
        let mut k = low;
        for _ in 0..4097 {
            meter(1).map_err(Failure::Operational)?;
            sum += binomial_log_mass(n, k, p).exp();
            if k == high {
                return Ok(sum.min(1.0));
            }
            k += 1.0;
        }
        return Err(Failure::ComputationalLimit);
    }
    if low == 0.0 {
        return binomial(high, n, p, true, meter);
    }
    if high == n {
        return tails::beta(
            low,
            n - low + 1.0,
            BetaCoordinate::Lower(p),
            Tail::Lower,
            meter,
        );
    }
    // On either side of the mean subtract small probabilities rather than values near one.
    if low > n * p {
        let left = tails::beta(
            low,
            n - low + 1.0,
            BetaCoordinate::Lower(p),
            Tail::Lower,
            &mut meter,
        )?;
        let right = tails::beta(
            high + 1.0,
            n - high,
            BetaCoordinate::Lower(p),
            Tail::Lower,
            meter,
        )?;
        Ok((left - right).max(0.0))
    } else {
        Ok(
            (binomial(high, n, p, true, &mut meter)? - binomial(low - 1.0, n, p, true, meter)?)
                .max(0.0),
        )
    }
}
/// Negative-binomial mass/CDF for failures before a positive number of successes.
pub fn negative_binomial<O>(
    failures: f64,
    successes: f64,
    p: f64,
    cumulative: bool,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[failures, successes, p])?;
    let k = bounded_count(failures)?;
    let r = bounded_count(successes)?;
    if r < 1.0 || !(0.0..=1.0).contains(&p) {
        return Err(num());
    }
    if p == 0.0 {
        return Ok(0.0);
    }
    if p == 1.0 {
        return Ok(f64::from(cumulative || k == 0.0));
    }
    if cumulative {
        return tails::beta(r, k + 1.0, BetaCoordinate::Lower(p), Tail::Lower, meter);
    }
    if k == 0.0 {
        return Ok((r * p.ln()).exp());
    }
    // C(k+r-1,k)*p^r*q^k = p^r*q^k/(k*B(r,k)).
    Ok((tails::beta_log_weight(r, k, p, 1.0 - p) - k.ln()).exp())
}
/// Hypergeometric mass/CDF with a metered finite support, without factorial products.
pub fn hypergeometric<O>(
    k: f64,
    n: f64,
    successes: f64,
    population: f64,
    cumulative: bool,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    inputs(&[k, n, successes, population])?;
    let k = count(k)?;
    let n = count(n)?;
    let successes = count(successes)?;
    let population = count(population)?;
    if population < 1.0
        || n < 1.0
        || n > population
        || successes < 1.0
        || successes > population
        || k > n.min(successes)
        || k < (n - (population - successes)).max(0.0)
    {
        return Err(num());
    }
    if population > 1e8 {
        return Err(Failure::ComputationalLimit);
    }
    if n == population {
        return Ok(1.0);
    }
    let p = n / population;
    let mass = |j: f64| {
        (binomial_log_mass(successes, j, p) + binomial_log_mass(population - successes, n - j, p)
            - binomial_log_mass(population, n, p))
        .exp()
    };
    if !cumulative {
        meter(1).map_err(Failure::Operational)?;
        return Ok(mass(k).min(1.0));
    }
    let low = (n - (population - successes)).max(0.0);
    let high = n.min(successes);
    if k == high {
        return Ok(1.0);
    }
    // Choose by the mean so a representable small lower tail is never 1-rounded.
    let upper = k >= n * (successes / population);
    let (start, end) = if upper { (k + 1.0, high) } else { (low, k) };
    if end - start > 100_000.0 {
        return Err(Failure::ComputationalLimit);
    }
    let mut sum = 0.0;
    let mut correction = 0.0;
    let mut j = start;
    for _ in 0..=100_000 {
        meter(1).map_err(Failure::Operational)?;
        let value = mass(j) - correction;
        let next = sum + value;
        correction = (next - sum) - value;
        sum = next;
        if j == end {
            // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
            return Ok(if upper { 1.0 - sum } else { sum }.clamp(0.0, 1.0));
        }
        j += 1.0;
    }
    Err(Failure::ComputationalLimit)
}
#[cfg(test)]
#[path = "modern_tests.rs"]
mod tests;
