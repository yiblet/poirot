//! Bounded inverse distributions and confidence margins; see INVERSE_SOURCES.md.
use super::modern::{self, Failure, Result};
use super::modern_tails::{self as tails, BetaCoordinate, Tail};
use poirot_functions_core::CellError;

fn num<O>() -> Failure<O> {
    Failure::Spreadsheet(CellError::Num)
}
fn admit<O>(values: &[f64]) -> std::result::Result<(), Failure<O>> {
    if values.iter().all(|x| x.is_finite()) {
        Ok(())
    } else {
        Err(num())
    }
}
fn finite<O>(x: f64) -> Result<O> {
    if x.is_finite() { Ok(x) } else { Err(num()) }
}
/// Normal quantile retaining the supplied smaller probability rather than 2*p-1.
fn normal(p: f64) -> f64 {
    let q = p.min(1.0 - p);
    let z = std::f64::consts::SQRT_2 * statrs::function::erf::erfc_inv(2.0 * q);
    if p < 0.5 { -z } else { z }
}
/// Log endpoint distances, retained until the worksheet's final transformation.
#[derive(Clone, Copy, Debug)]
struct BetaPoint {
    lower: f64,
    upper: f64,
}
impl BetaPoint {
    /// Construct endpoint logs from a log odds coordinate without overflowing exp.
    fn from_odds(t: f64) -> Self {
        if t <= 0.0 {
            let s = t.exp().ln_1p();
            Self {
                lower: t - s,
                upper: -s,
            }
        } else {
            let s = (-t).exp().ln_1p();
            Self {
                lower: -s,
                upper: -t - s,
            }
        }
    }
    /// Evaluate the coordinate from the more accurate endpoint distance.
    fn coordinate(self) -> BetaCoordinate {
        if self.lower < self.upper {
            BetaCoordinate::LowerLog(self.lower)
        } else {
            BetaCoordinate::UpperLog(self.upper)
        }
    }
    /// Reflect around one half without subtracting a rounded coordinate.
    fn reflect(self) -> Self {
        Self {
            lower: self.upper,
            upper: self.lower,
        }
    }
    /// Project only at the final affine worksheet boundary.
    fn value(self) -> f64 {
        if self.lower < self.upper {
            self.lower.exp()
        } else {
            -self.upper.exp_m1()
        }
    }
}
/// Invert the smaller beta tail in log odds; all nested evaluations are charged.
fn beta_point<O>(
    p: f64,
    a: f64,
    b: f64,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<BetaPoint, Failure<O>> {
    beta_point_bounded(p, a, b, 64, meter)
}
/// Refine within the worksheet's finite search allowance, including bracketing.
fn beta_point_bounded<O>(
    p: f64,
    a: f64,
    b: f64,
    max_iterations: usize,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<BetaPoint, Failure<O>> {
    if a > 1e12 || b > 1e12 {
        return Err(Failure::ComputationalLimit);
    }
    meter(1).map_err(Failure::Operational)?;
    if p > 0.5 {
        return Ok(beta_point_bounded(1.0 - p, b, a, max_iterations, meter)?.reflect());
    }
    if b == 1.0 {
        let lower = p.ln() / a;
        return Ok(BetaPoint {
            lower,
            upper: (-lower.exp_m1()).ln(),
        });
    }
    if a == 1.0 {
        let upper = (-p).ln_1p() / b;
        return Ok(BetaPoint {
            lower: (-upper.exp_m1()).ln(),
            upper,
        });
    }
    if a == b && p == 0.5 {
        return Ok(BetaPoint::from_odds(0.0));
    }
    let mean = a / (a + b);
    let guess = mean + normal(p) * (mean * (1.0 - mean) / (a + b + 1.0)).sqrt();
    let mut t = if a <= 1.0 || guess <= 0.0 {
        let log_x = (p.ln() + a.ln() + tails::log_beta(a, b)) / a;
        if log_x < -0.1 {
            log_x - (-log_x.exp()).ln_1p()
        } else {
            (a / b).ln()
        }
    } else if guess < 1.0 {
        guess.ln() - (-guess).ln_1p()
    } else {
        (a / b).ln()
    };
    if !t.is_finite() {
        return Err(Failure::NonConvergence);
    }
    let mut low = None;
    let mut high = None;
    let mut span = 1.0;
    for _ in 0..max_iterations {
        meter(1).map_err(Failure::Operational)?;
        let point = BetaPoint::from_odds(t);
        let value = tails::beta(a, b, point.coordinate(), Tail::Lower, &mut meter)?;
        let centered = a == b && a <= 1.0 && t.abs() <= 1.0;
        let error = if centered {
            tails::beta_center_offset(a, t, &mut meter)? - (p - 0.5)
        } else {
            value - p
        };
        let derivative = tails::beta_log_weight_logs(a, b, point.lower, point.upper).exp();
        let probability_scale = if centered { (p - 0.5).abs() } else { p };
        if error.abs() <= probability_scale * 2e-13
            && error.abs() / derivative <= 4e-13 * t.abs().max(1.0)
        {
            return Ok(point);
        }
        if error < 0.0 {
            low = Some((t, error.abs()));
        } else {
            high = Some((t, error.abs()));
        }
        t = match (low, high) {
            (Some((l, le)), Some((h, he))) => {
                // A containing bracket whose log odds are adjacent bounds the ratio
                // to one log-coordinate ULP, including unrepresentable beta endpoints.
                if l.next_up() >= h {
                    return Ok(BetaPoint::from_odds(if le <= he { l } else { h }));
                }
                let derivative = tails::beta_log_weight_logs(a, b, point.lower, point.upper).exp();
                let candidate = t - error / derivative;
                if candidate.is_finite() && candidate > l && candidate < h && candidate != t {
                    candidate
                } else {
                    l * 0.5 + h * 0.5
                }
            }
            (Some((l, _)), None) => {
                let next = l + span;
                span *= 2.0;
                next
            }
            (None, Some((h, _))) => {
                let next = h - span;
                span *= 2.0;
                next
            }
            _ => return Err(Failure::NonConvergence),
        };
        if !t.is_finite() {
            return Err(Failure::NonConvergence);
        }
    }
    Err(Failure::NonConvergence)
}
/// Multiply a finite value by a retained endpoint distance, recovering scale if needed.
fn scaled(value: f64, log_distance: f64) -> f64 {
    if value == 0.0 {
        return value;
    }
    let distance = log_distance.exp();
    let ordinary = value * distance;
    if distance.is_normal() && ordinary.is_normal() {
        ordinary
    } else {
        (value.abs().ln() + log_distance).exp().copysign(value)
    }
}
/// Beta quantile on independently optional finite lower/upper bounds.
pub fn beta<O>(
    p: f64,
    a: f64,
    b: f64,
    lower: f64,
    upper: f64,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[p, a, b, lower, upper])?;
    if p <= 0.0 || p > 1.0 || a <= 0.0 || b <= 0.0 || lower >= upper {
        return Err(num());
    }
    if p == 1.0 {
        return Ok(upper);
    }
    let point = beta_point(p, a, b, meter)?;
    let x = point.value();
    // Weighted interpolation avoids the overflow of upper-lower for opposite signs.
    let result = if lower.signum() != upper.signum() {
        scaled(lower, point.upper) + scaled(upper, point.lower)
    } else if x <= 0.5 {
        lower + scaled(upper - lower, point.lower)
    } else {
        upper - scaled(upper - lower, point.upper)
    };
    finite(result)
}
/// Dimensionless gamma coordinate retaining a log when its value underflows.
struct GammaPoint {
    value: f64,
    log: f64,
}
impl GammaPoint {
    /// Retain an ordinary coordinate without a logarithm/exponential round trip.
    fn value(value: f64) -> Self {
        Self {
            value,
            log: value.ln(),
        }
    }
    /// Retain a small coordinate until worksheet scale is applied.
    fn logarithm(log: f64) -> Self {
        Self {
            value: log.exp(),
            log,
        }
    }
}
/// Gamma quantile in dimensionless coordinates, retaining a log for scale recovery.
fn gamma_log<O>(
    p: f64,
    a: f64,
    tail: Tail,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<GammaPoint, Failure<O>> {
    if a > 1e12 {
        return Err(Failure::ComputationalLimit);
    }
    meter(1).map_err(Failure::Operational)?;
    if a == 1.0 {
        return Ok(GammaPoint::value(if tail == Tail::Lower {
            -(-p).ln_1p()
        } else {
            -p.ln()
        }));
    }
    let (p, tail) = if p > 0.5 {
        (
            1.0 - p,
            if tail == Tail::Lower {
                Tail::Upper
            } else {
                Tail::Lower
            },
        )
    } else {
        (p, tail)
    };
    let log_lower = if tail == Tail::Lower {
        p.ln()
    } else {
        (-p).ln_1p()
    };
    let small_log = (log_lower + tails::log_gamma_one_plus(a)) / a;
    // Write x0=exp(small_log). Integrating exp(-t) between exp(-x) and 1
    // gives x>=x0. If x0/a<5e-16, P(2*x0)>=p since 2*x0<a*ln(2),
    // so x<=2*x0 and log(x/x0)<=2*x0/a<1e-15. Compare logs before
    // projecting x0, which may itself be a zero/subnormal binary64 value.
    if small_log < -32.0 && small_log - a.ln() < (5e-16_f64).ln() {
        return Ok(GammaPoint::logarithm(small_log));
    }
    let z = if tail == Tail::Lower {
        normal(p)
    } else {
        -normal(p)
    };
    let w = 1.0 - 1.0 / (9.0 * a) + z / (3.0 * a.sqrt());
    let mut x = if a > 1.0 && w > 0.0 {
        a * w * w * w
    } else if tail == Tail::Lower {
        small_log.exp()
    } else {
        a.max(-p.ln())
    };
    if !x.is_finite() || x <= 0.0 {
        return Err(Failure::NonConvergence);
    }
    let mut low = 0.0;
    let mut high = f64::INFINITY;
    let mut low_error = if tail == Tail::Lower { p } else { 1.0 - p };
    let mut high_error = f64::INFINITY;
    for _ in 0..64 {
        meter(1).map_err(Failure::Operational)?;
        let value = tails::gamma(a, x, tail, &mut meter)?;
        let error = value - p;
        let density = if a >= 128.0 {
            tails::gamma_log_density(a, x)
        } else {
            (a - 1.0) * x.ln() - x - libm::lgamma(a)
        }
        .exp();
        if error.abs() <= p * 2e-13 && error.abs() / density <= x * 4e-13 {
            return Ok(GammaPoint::value(x));
        }
        let signed = if tail == Tail::Lower { error } else { -error };
        if signed < 0.0 {
            low = x;
            low_error = error.abs();
        } else {
            high = x;
            high_error = error.abs();
        }
        if low.next_up() >= high {
            return Ok(GammaPoint::value(if low_error <= high_error {
                low
            } else {
                high
            }));
        }
        let candidate = x - signed / density;
        x = if candidate.is_finite()
            && candidate > low
            && candidate < high
            && candidate != x
            && (high.is_finite() || candidate <= x * 2.0)
        {
            candidate
        } else if high.is_finite() {
            low * 0.5 + high * 0.5
        } else {
            x * 2.0
        };
        if !x.is_finite() || x <= 0.0 {
            return Err(Failure::NonConvergence);
        }
    }
    Err(Failure::NonConvergence)
}
/// Gamma inverse with a positive scale and a selected probability tail.
pub fn gamma<O>(
    p: f64,
    a: f64,
    scale: f64,
    tail: Tail,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[p, a, scale])?;
    if !(0.0..=1.0).contains(&p) || a <= 0.0 || scale <= 0.0 {
        return Err(num());
    }
    if (p == 0.0 && tail == Tail::Lower) || (p == 1.0 && tail == Tail::Upper) {
        return Ok(0.0);
    }
    if p == 0.0 || p == 1.0 {
        return Err(num());
    }
    let point = gamma_log(p, a, tail, meter)?;
    let ordinary = point.value * scale;
    finite(if point.value.is_normal() && ordinary.is_normal() {
        ordinary
    } else {
        (point.log + scale.ln()).exp()
    })
}
/// Chi-square inverse preserving the different documented degree domains by tail.
pub fn chi_square<O>(
    p: f64,
    degrees: f64,
    tail: Tail,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[p, degrees])?;
    let df = degrees.trunc();
    if df < 1.0 || (tail == Tail::Lower && df > 1e10) {
        return Err(num());
    }
    gamma(p, df * 0.5, 2.0, tail, meter)
}
/// Fisher inverse mapping retained beta log odds directly to a finite ratio.
pub fn fisher_f<O>(
    p: f64,
    d1: f64,
    d2: f64,
    tail: Tail,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[p, d1, d2])?;
    let (d1, d2) = (d1.trunc(), d2.trunc());
    if !(0.0..=1.0).contains(&p) || d1 < 1.0 || d2 < 1.0 || (tail == Tail::Upper && d2 >= 1e10) {
        return Err(num());
    }
    if (p == 0.0 && tail == Tail::Lower) || (p == 1.0 && tail == Tail::Upper) {
        return Ok(0.0);
    }
    if p == 0.0 || p == 1.0 {
        return Err(num());
    }
    let point = if tail == Tail::Lower {
        beta_point(p, d1 * 0.5, d2 * 0.5, meter)?
    } else {
        beta_point(p, d2 * 0.5, d1 * 0.5, meter)?.reflect()
    };
    finite((point.lower - point.upper + d2.ln() - d1.ln()).exp())
}
/// Binomial smallest qualifying integer, searched only within its finite support.
pub fn binomial<O>(
    n: f64,
    p: f64,
    alpha: f64,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[n, p, alpha])?;
    if n < 0.0 || p <= 0.0 || p >= 1.0 || alpha <= 0.0 || alpha >= 1.0 {
        return Err(num());
    }
    let n = n.trunc();
    if n > 1e12 {
        return Err(Failure::ComputationalLimit);
    }
    let (mut low, mut high) = (0.0, n);
    for _ in 0..41 {
        meter(1).map_err(Failure::Operational)?;
        if low == high {
            return Ok(low);
        }
        let middle = low + ((high - low) * 0.5).floor();
        if modern::binomial(middle, n, p, true, &mut meter)? >= alpha {
            high = middle;
        } else {
            low = middle + 1.0;
        }
    }
    Err(Failure::NonConvergence)
}
/// Student t inverse, using beta endpoint logs for finite extreme quantiles.
pub fn student_t<O>(
    p: f64,
    degrees: f64,
    two_tail: bool,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[p, degrees])?;
    let df = degrees.trunc();
    if p <= 0.0 || p > 1.0 || df < 1.0 {
        return Err(num());
    }
    if two_tail && p == 1.0 || !two_tail && p == 0.5 {
        return Ok(0.0);
    }
    if !two_tail && p == 1.0 {
        return Err(num());
    }
    let probability = if two_tail { p } else { 2.0 * p.min(1.0 - p) };
    let t = student_log(probability, df, meter)?.exp();
    finite(if !two_tail && p < 0.5 { -t } else { t })
}
/// Positive t critical value in logarithmic coordinates for final confidence scaling.
fn student_log<O>(
    probability: f64,
    df: f64,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    let point = beta_point(probability, df * 0.5, 0.5, meter)?;
    Ok(0.5 * (df.ln() + point.upper - point.lower))
}
/// Lognormal inverse using the stable normal smaller-tail quantile.
pub fn lognormal<O>(p: f64, mean: f64, sigma: f64) -> Result<O> {
    admit(&[p, mean, sigma])?;
    if p <= 0.0 || p >= 1.0 || sigma <= 0.0 {
        return Err(num());
    }
    finite(sigma.mul_add(normal(p), mean).exp())
}
/// Sampling distribution defining the confidence critical value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfidenceKind {
    /// Known population standard deviation and normal sampling distribution.
    Normal,
    /// Estimated standard deviation and Student t sampling distribution.
    Student,
}
/// Confidence margin for a normal or Student t sampling distribution.
pub fn confidence<O>(
    alpha: f64,
    sigma: f64,
    size: f64,
    kind: ConfidenceKind,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    admit(&[alpha, sigma, size])?;
    let n = size.trunc();
    if alpha <= 0.0 || alpha >= 1.0 || sigma <= 0.0 || n < 1.0 {
        return Err(num());
    }
    if kind == ConfidenceKind::Student && n == 1.0 {
        return Err(Failure::Spreadsheet(CellError::Div0));
    }
    let log_critical = if kind == ConfidenceKind::Student {
        student_log(alpha, n - 1.0, meter)?
    } else {
        (std::f64::consts::SQRT_2 * statrs::function::erf::erfc_inv(alpha)).ln()
    };
    let critical = log_critical.exp();
    let standard_error = sigma / n.sqrt();
    let margin = critical * standard_error;
    finite(
        if critical.is_normal() && standard_error.is_normal() && margin.is_normal() {
            margin
        } else {
            (log_critical + sigma.ln() - 0.5 * n.ln()).exp()
        },
    )
}

#[cfg(test)]
#[path = "modern_inverse_tests.rs"]
mod tests;
