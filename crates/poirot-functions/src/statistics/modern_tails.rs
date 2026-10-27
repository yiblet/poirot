//! Bounded modern beta/gamma tails, independent of pinned numerical policies.
//! Equations and independently generated quadrature constants: DISTRIBUTION_SOURCES.md.
use super::distribution_error::DistributionError;
use super::modern::Result;
use crate::scalar::CellError;

/// Probability side retained through endpoint-sensitive transformations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tail {
    /// Integral from the lower support endpoint.
    Lower,
    /// Integral to the upper support endpoint.
    Upper,
}
/// Beta coordinate retaining a supplied small distance from either endpoint.
#[derive(Clone, Copy, Debug)]
pub enum BetaCoordinate {
    /// Ordinary coordinate measured from zero.
    Lower(f64),
    /// Distance from one, without first rounding one minus that distance.
    Upper(f64),
    /// Logarithm of the coordinate measured from zero.
    LowerLog(f64),
    /// Logarithm of the distance from one.
    UpperLog(f64),
}
const LOG_SQRT_TAU: f64 = 0.9189385332046727;
const LIMIT: usize = 100_000;
fn charge<O>(
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> std::result::Result<(), DistributionError<O>> {
    meter(1).map_err(DistributionError::Operational)
}
/// Evaluate log(1+u)-u without losing the quadratic term near zero.
fn log1pmx(u: f64) -> f64 {
    if u.abs() > 0.125 {
        return u.ln_1p() - u;
    }
    let mut power = u * u;
    let mut sum = 0.0;
    // The geometric remainder at |u| <=1/8 is < |u|^33/(33*(1-|u|)).
    for k in 2..33 {
        // SAFETY: The literal divisor 2 is nonzero and cannot cause MIN % -1.
        sum += if k % 2 == 0 {
            -power / (k as f64)
        } else {
            power / (k as f64)
        };
        power *= u;
    }
    sum
}
/// Stirling's alternating correction through a^-15; next term bounds the error.
fn correction(a: f64) -> f64 {
    let q = 1.0 / a;
    let v = q * q;
    q * (1.0 / 12.0
        + v * (-1.0 / 360.0
            + v * (1.0 / 1260.0
                + v * (-1.0 / 1680.0
                    + v * (1.0 / 1188.0
                        + v * (-691.0 / 360360.0 + v * (1.0 / 156.0 - v * 3617.0 / 122400.0)))))))
}
/// Stable logarithm of the beta normalization for positive finite shapes.
pub(crate) fn log_beta(a: f64, b: f64) -> f64 {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    if b < 8.0 {
        return libm::lgamma(a) + libm::lgamma(b) - libm::lgamma(a + b);
    }
    if a < 8.0 {
        let u = a / b;
        let ratio =
            a * b.ln() + b * log1pmx(u) + (a - 0.5) * u.ln_1p() + correction(a + b) - correction(b);
        return libm::lgamma(a) - ratio;
    }
    let s = a + b;
    let p = a / s;
    let q = b / s;
    a * p.ln() + b * q.ln() - 0.5 * (a.ln() + b.ln() - s.ln())
        + LOG_SQRT_TAU
        + correction(a)
        + correction(b)
        - correction(s)
}
/// Logarithm of x^a*(1-x)^b/B(a,b), preserving large-shape cancellation.
pub(crate) fn beta_log_weight(a: f64, b: f64, x: f64, y: f64) -> f64 {
    if a >= 8.0 && b >= 8.0 {
        let s = a + b;
        let p = a / s;
        let q = b / s;
        // Use the smaller supplied coordinate to retain its endpoint precision.
        let error = if a >= b { (a - s) + b } else { (b - s) + a };
        let delta = if x <= y {
            x.mul_add(s, -a) + x * error
        } else {
            (-y).mul_add(s, b) - y * error
        } / s;
        a * log1pmx(delta / p) + b * log1pmx(-delta / q) + 0.5 * (a.ln() + b.ln() - s.ln())
            - LOG_SQRT_TAU
            - correction(a)
            - correction(b)
            + correction(s)
    } else {
        let (log_x, log_y) = if x <= y {
            (x.ln(), (-x).ln_1p())
        } else {
            ((-y).ln_1p(), y.ln())
        };
        a * log_x + b * log_y - log_beta(a, b)
    }
}
/// Regularized incomplete beta with explicit tail and retained endpoint distance.
pub fn beta<O>(
    a: f64,
    b: f64,
    coordinate: BetaCoordinate,
    tail: Tail,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    let (a, b, x, tail) = match coordinate {
        BetaCoordinate::LowerLog(log) | BetaCoordinate::UpperLog(log) => {
            let upper = matches!(coordinate, BetaCoordinate::UpperLog(_));
            let (a, b, tail) = if upper {
                (
                    b,
                    a,
                    if tail == Tail::Lower {
                        Tail::Upper
                    } else {
                        Tail::Lower
                    },
                )
            } else {
                (a, b, tail)
            };
            if log < -230.0 && a > 0.0 && b > 0.0 && a.is_finite() && b.is_finite() {
                if a > 1e12 || b > 1e12 {
                    return Err(DistributionError::ComputationalLimit);
                }
                charge(&mut meter)?;
                if b == 1.0 {
                    let value = a * log;
                    return Ok(if tail == Tail::Lower {
                        value.exp()
                    } else {
                        -value.exp_m1()
                    });
                }
                if a == 1.0 {
                    let value = log + b.ln();
                    return Ok(if tail == Tail::Lower {
                        value.exp()
                    } else {
                        -value.exp_m1()
                    });
                }
                if a < 0.125 && b >= a && -a * log < 0.1 {
                    return beta_small_shape(a, b, log, tail, &mut meter);
                }
                let value = a * log - log_beta(a, b) - a.ln();
                return Ok(if tail == Tail::Lower {
                    value.exp()
                } else {
                    -value.exp_m1()
                }
                // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
                .clamp(0.0, 1.0));
            }
            return beta(a, b, BetaCoordinate::Lower(log.exp()), tail, meter);
        }
        BetaCoordinate::Lower(x) => (a, b, x, tail),
        BetaCoordinate::Upper(y) => (
            b,
            a,
            y,
            if tail == Tail::Lower {
                Tail::Upper
            } else {
                Tail::Lower
            },
        ),
    };
    if !a.is_finite()
        || !b.is_finite()
        || a <= 0.0
        || b <= 0.0
        || !x.is_finite()
        || !(0.0..=1.0).contains(&x)
    {
        return Err(DistributionError::Spreadsheet(CellError::Num));
    }
    if x == 0.0 {
        return Ok(if tail == Tail::Lower { 0.0 } else { 1.0 });
    }
    if x == 1.0 {
        return Ok(if tail == Tail::Lower { 1.0 } else { 0.0 });
    }
    if a > 1e12 || b > 1e12 {
        return Err(DistributionError::ComputationalLimit);
    }
    charge(&mut meter)?;
    if a == b && x == 0.5 {
        return Ok(0.5);
    }
    if a >= 1e6 && b >= 1e6 {
        return beta_integral(a, b, x, tail, &mut meter);
    }
    if b == 1.0 {
        let l = a * x.ln();
        return Ok(if tail == Tail::Lower {
            l.exp()
        } else {
            -l.exp_m1()
        });
    }
    if a == 1.0 {
        let l = b * (-x).ln_1p();
        return Ok(if tail == Tail::Upper {
            l.exp()
        } else {
            -l.exp_m1()
        });
    }
    if a < 0.125 && b >= a && -a * x.ln() < 0.1 {
        return beta_small_shape(a, b, x.ln(), tail, &mut meter);
    }
    if b < 0.125 && a >= b && -b * (-x).ln_1p() < 0.1 {
        return beta_small_shape(
            b,
            a,
            (-x).ln_1p(),
            if tail == Tail::Lower {
                Tail::Upper
            } else {
                Tail::Lower
            },
            &mut meter,
        );
    }
    // Reflection makes the fraction converge towards the nearer endpoint.
    let reflected = x > (a + 1.0) / (a + b + 2.0);
    let (a, b, x, y) = if reflected {
        (b, a, 1.0 - x, x)
    } else {
        (a, b, x, 1.0 - x)
    };
    let tiny = 1e-300;
    let floor = |v: f64| if v.abs() < tiny { tiny.copysign(v) } else { v };
    let mut c = 1.0;
    let mut d = 1.0 / floor(1.0 - (a + b) * x / (a + 1.0));
    let mut h = d;
    for m in 1..=LIMIT {
        charge(&mut meter)?;
        let m = m as f64;
        let twice = 2.0 * m;
        let aa = m * (b - m) * x / ((a + twice - 1.0) * (a + twice));
        d = 1.0 / floor(1.0 + aa * d);
        c = floor(1.0 + aa / c);
        h *= d * c;
        let aa = -(a + m) * (a + b + m) * x / ((a + twice) * (a + twice + 1.0));
        d = 1.0 / floor(1.0 + aa * d);
        c = floor(1.0 + aa / c);
        let delta = d * c;
        h *= delta;
        if !h.is_finite() || h <= 0.0 {
            return Err(DistributionError::NonConvergence);
        }
        if (delta - 1.0).abs() <= 4.0 * f64::EPSILON {
            let log = beta_log_weight(a, b, x, y) + h.ln() - a.ln();
            let direct = (tail == Tail::Lower) != reflected;
            // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
            return Ok(if direct { log.exp() } else { -log.exp_m1() }.clamp(0.0, 1.0));
        }
    }
    Err(DistributionError::NonConvergence)
}
/// Logarithm of the gamma density after standardizing x=a+t*sqrt(a).
fn standardized_log_density(a: f64, t: f64) -> f64 {
    let u = t / a.sqrt();
    if u <= -1.0 {
        return f64::NEG_INFINITY;
    }
    a * log1pmx(u) - u.ln_1p() - LOG_SQRT_TAU - correction(a)
}
// Independently generated from Legendre polynomial roots at 90 decimal digits.
const G8: [(f64, f64); 4] = [
    (0.9602898564975363, 0.10122853629037626),
    (0.7966664774136267, 0.22238103445337448),
    (0.525532409916329, 0.31370664587788727),
    (0.1834346424956498, 0.362683783378362),
];
const G16: [(f64, f64); 8] = [
    (0.9894009349916499, 0.027152459411754096),
    (0.9445750230732326, 0.062253523938647894),
    (0.8656312023878318, 0.09515851168249278),
    (0.755404408355003, 0.12462897125553388),
    (0.6178762444026438, 0.14959598881657674),
    (0.45801677765722737, 0.16915651939500254),
    (0.2816035507792589, 0.18260341504492358),
    (0.09501250983763744, 0.1894506104550685),
];
/// Adaptive quadrature state for beta and gamma tail probabilities.
/// It rescales the log density and shares a finite node allowance across subdivisions.
struct TailIntegral<F> {
    log_density: F,
    scale: f64,
    remaining: usize,
}
impl<F: Fn(f64) -> f64> TailIntegral<F> {
    fn rule<O>(
        &mut self,
        l: f64,
        r: f64,
        nodes: &[(f64, f64)],
        meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> Result<O> {
        let mid = l + (r - l) * 0.5;
        let half = (r - l) * 0.5;
        let mut sum = 0.0;
        for &(x, w) in nodes {
            for t in [mid - half * x, mid + half * x] {
                if self.remaining == 0 {
                    return Err(DistributionError::ComputationalLimit);
                }
                self.remaining = self
                    .remaining
                    .checked_sub(1)
                    .ok_or(DistributionError::ComputationalLimit)?;
                charge(meter)?;
                sum += w * ((self.log_density)(t) - self.scale).exp();
            }
        }
        Ok(sum * half)
    }
    fn integrate<O>(
        &mut self,
        l: f64,
        r: f64,
        tolerance: f64,
        depth: u8,
        meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
    ) -> Result<O> {
        let coarse = self.rule(l, r, &G8, meter)?;
        let fine = self.rule(l, r, &G16, meter)?;
        if !fine.is_finite() {
            return Err(DistributionError::NonConvergence);
        }
        if (fine - coarse).abs() <= tolerance.max(fine.abs() * 512.0 * f64::EPSILON) {
            return Ok(fine);
        }
        if depth == 0 {
            return Err(DistributionError::NonConvergence);
        }
        let mid = l + (r - l) * 0.5;
        if mid == l || mid == r {
            return Err(DistributionError::NonConvergence);
        }
        let next = depth
            .checked_sub(1)
            .ok_or(DistributionError::NonConvergence)?;
        Ok(self.integrate(l, mid, tolerance * 0.5, next, meter)?
            + self.integrate(mid, r, tolerance * 0.5, next, meter)?)
    }
}
/// Gamma tails for large shapes through the actual standardized-density integral.
fn gamma_integral<O>(
    a: f64,
    x: f64,
    tail: Tail,
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    let sq = a.sqrt();
    let z = (x - a) / sq;
    let upper = z >= 0.0;
    // Chernoff: the selected tail <= exp(a*(log(x/a)+1-x/a)).
    // exp(-800) is smaller than half the smallest binary64 subnormal.
    let deviance = a * log1pmx((x - a) / a);
    if deviance < -800.0 {
        return Ok(if (tail == Tail::Upper) == upper {
            0.0
        } else {
            1.0
        });
    }
    let mut right = 1.0;
    for _ in 0..32 {
        charge(meter)?;
        if a * log1pmx(right / sq) < -800.0 {
            break;
        }
        right *= 2.0;
    }
    if a * log1pmx(right / sq) >= -800.0 {
        return Err(DistributionError::ComputationalLimit);
    }
    let mut left = -1.0f64;
    for _ in 0..32 {
        charge(meter)?;
        if left <= -sq || a * log1pmx(left / sq) < -800.0 {
            break;
        }
        left *= 2.0;
    }
    left = left.max(-sq);
    let (l, r) = if upper { (z, right) } else { (left, z) };
    let scale = standardized_log_density(a, if upper { z } else { z.min(-1.0 / sq) });
    let mut integral = TailIntegral {
        log_density: |t| standardized_log_density(a, t),
        scale,
        remaining: LIMIT,
    };
    let coarse = integral.rule(l, r, &G16, meter)?;
    if coarse <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let value = integral.integrate(l, r, coarse * 2e-13, 20, meter)?;
    if value <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let log = scale + value.ln();
    Ok(if (tail == Tail::Upper) == upper {
        log.exp()
    } else {
        -log.exp_m1()
    }
    // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
    .clamp(0.0, 1.0))
}
/// Regularized incomplete gamma with bounded, charged series/fraction/integration.
pub fn gamma<O>(
    a: f64,
    x: f64,
    tail: Tail,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    if !a.is_finite() || a <= 0.0 || x.is_nan() || x < 0.0 {
        return Err(DistributionError::Spreadsheet(CellError::Num));
    }
    if x == 0.0 {
        return Ok(if tail == Tail::Lower { 0.0 } else { 1.0 });
    }
    if x == f64::INFINITY {
        return Ok(if tail == Tail::Lower { 1.0 } else { 0.0 });
    }
    charge(&mut meter)?;
    if a == 1.0 {
        return Ok(if tail == Tail::Lower {
            -(-x).exp_m1()
        } else {
            (-x).exp()
        });
    }
    if a > 1e12 {
        return Err(DistributionError::ComputationalLimit);
    }
    if a >= 4096.0 {
        return gamma_integral(a, x, tail, &mut meter);
    }
    let u = (x - a) / a;
    let log = if a >= 128.0 && u.abs() < 0.125 {
        a * log1pmx(u) + 0.5 * a.ln() - LOG_SQRT_TAU - correction(a)
    } else {
        a * x.ln() - x - libm::lgamma(a)
    };
    if a < 1.0 && x < 1.0 {
        // Integrate exp(-t)'s alternating series before subtracting from one.
        // Q = -expm1(a*ln(x)-lnGamma(1+a)) - exp(log)*sum_{n>=1} (-x)^n*a/(n!*(a+n)).
        let log = a * x.ln() - log_gamma_one_plus(a);
        let mut factor = 1.0;
        let mut sum = 0.0;
        for n in 1..=LIMIT {
            charge(&mut meter)?;
            factor *= -x / (n as f64);
            let term = factor * a / (a + n as f64);
            sum += term;
            let q = -log.exp_m1() - log.exp() * sum;
            if term.abs() <= q.abs() * f64::EPSILON || term == 0.0 {
                return Ok(if tail == Tail::Upper {
                    q
                } else {
                    (log + sum.ln_1p()).exp()
                }
                // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
                .clamp(0.0, 1.0));
            }
        }
        return Err(DistributionError::NonConvergence);
    }
    if x < a + 1.0 && a >= 1.0 {
        let mut term = 1.0;
        let mut sum = 1.0;
        for n in 1..=LIMIT {
            charge(&mut meter)?;
            term *= x / (a + n as f64);
            sum += term;
            if !sum.is_finite() {
                return Err(DistributionError::NonConvergence);
            }
            if term <= sum * 2.0 * f64::EPSILON {
                let l = log - a.ln() + sum.ln();
                return Ok(if tail == Tail::Lower {
                    l.exp()
                } else {
                    -l.exp_m1()
                }
                // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
                .clamp(0.0, 1.0));
            }
        }
    } else {
        let tiny = 1e-300;
        let floor = |v: f64| if v.abs() < tiny { tiny.copysign(v) } else { v };
        let mut b = x + 1.0 - a;
        let mut c = 1.0 / tiny;
        let mut d = 1.0 / floor(b);
        let mut h = d;
        for n in 1..=LIMIT {
            charge(&mut meter)?;
            let n = n as f64;
            let an = n * (a - n);
            b += 2.0;
            d = 1.0 / floor(an * d + b);
            c = floor(b + an / c);
            let delta = d * c;
            h *= delta;
            if !h.is_finite() || h <= 0.0 {
                return Err(DistributionError::NonConvergence);
            }
            if (delta - 1.0).abs() <= 4.0 * f64::EPSILON {
                let l = log + h.ln();
                return Ok(if tail == Tail::Upper {
                    l.exp()
                } else {
                    -l.exp_m1()
                }
                // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
                .clamp(0.0, 1.0));
            }
        }
    }
    Err(DistributionError::NonConvergence)
}

/// Log gamma density with cancellation-safe large-shape evaluation.
pub(crate) fn gamma_log_density(a: f64, x: f64) -> f64 {
    standardized_log_density(a, (x - a) / a.sqrt()) - 0.5 * a.ln()
}

/// Log gamma near one, retaining the linear term when 1+a rounds to one.
pub(crate) fn log_gamma_one_plus(a: f64) -> f64 {
    if a >= 0.001 {
        return libm::lgamma(1.0 + a);
    }
    // Taylor coefficients -EulerGamma and alternating zeta(n)/n through n=6.
    a * (-0.5772156649015329
        + a * (0.8224670334241132
            + a * (-0.40068563438653143
                + a * (0.27058080842778455 + a * (-0.20738555102867398 + a * 0.1695571769974082)))))
}
/// Beta log weight from logarithmic endpoint coordinates when ratios underflow.
pub(crate) fn beta_log_weight_logs(a: f64, b: f64, log_x: f64, log_y: f64) -> f64 {
    if log_x < -230.0 || log_y < -230.0 {
        a * log_x + b * log_y - log_beta(a, b)
    } else {
        beta_log_weight(a, b, log_x.exp(), log_y.exp())
    }
}

/// Difference of a supplied endpoint coordinate from the exact shape ratio.
fn mean_difference(a: f64, b: f64, x: f64, y: f64) -> f64 {
    let s = a + b;
    let error = if a >= b { (a - s) + b } else { (b - s) + a };
    if x <= y {
        (x.mul_add(s, -a) + x * error) / s
    } else {
        ((-y).mul_add(s, b) - y * error) / s
    }
}
/// Large-shape beta tails through their standardized actual density.
fn beta_integral<O>(
    a: f64,
    b: f64,
    x: f64,
    tail: Tail,
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    let s = a + b;
    let p = a / s;
    let q = b / s;
    let sd = (p * q / (s + 1.0)).sqrt();
    let z = mean_difference(a, b, x, 1.0 - x) / sd;
    let upper = z >= 0.0;
    let deviance = |t: f64| {
        let u = sd / p * t;
        let v = -sd / q * t;
        if u <= -1.0 || v <= -1.0 {
            f64::NEG_INFINITY
        } else {
            a * log1pmx(u) + b * log1pmx(v)
        }
    };
    if deviance(z) < -800.0 {
        return Ok(if (tail == Tail::Upper) == upper {
            0.0
        } else {
            1.0
        });
    }
    let log_density = |t: f64| {
        deviance(t)
            - (sd / p * t).ln_1p()
            - (-sd / q * t).ln_1p()
            - LOG_SQRT_TAU
            - 0.5 * (1.0 / s).ln_1p()
            - correction(a)
            - correction(b)
            + correction(s)
    };
    let mut left = -1.0;
    let mut right = 1.0;
    for _ in 0..32 {
        charge(meter)?;
        if deviance(left) < -800.0 {
            break;
        }
        left *= 2.0;
    }
    for _ in 0..32 {
        charge(meter)?;
        if deviance(right) < -800.0 {
            break;
        }
        right *= 2.0;
    }
    left = left.max(-p / sd);
    right = right.min(q / sd);
    let (l, r) = if upper { (z, right) } else { (left, z) };
    let mode = (2.0 * p - 1.0) / (s - 2.0) / sd;
    let scale = log_density(if upper { z.max(mode) } else { z.min(mode) });
    let mut integral = TailIntegral {
        log_density,
        scale,
        remaining: LIMIT,
    };
    let coarse = integral.rule(l, r, &G16, meter)?;
    if coarse <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let value = integral.integrate(l, r, coarse * 2e-13, 20, meter)?;
    if value <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let log = scale + value.ln();
    Ok(if (tail == Tail::Upper) == upper {
        log.exp()
    } else {
        -log.exp_m1()
    }
    // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
    .clamp(0.0, 1.0))
}

/// Direct positive upper-beta integral when a tiny lower shape defeats complementation.
fn beta_small_shape<O>(
    a: f64,
    b: f64,
    log_x: f64,
    tail: Tail,
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    // t=exp(-s) gives Q=integral_0^-ln(x) exp(-a*s)*(1-exp(-s))^(b-1)/B(a,b) ds.
    // For b<1, u=s^b cancels the integrable singularity at s=0.
    let end = -log_x;
    if end > 4096.0 {
        return Err(DistributionError::ComputationalLimit);
    }
    let log_b = log_beta(a, b);
    let log_complement = if log_x > -std::f64::consts::LN_2 {
        (-log_x.exp_m1()).ln()
    } else {
        (-log_x.exp()).ln_1p()
    };
    // Since a<1, t^(a-1)<=x^(a-1) on [x,1]. Integrating (1-t)^(b-1)
    // gives Q<=x^(a-1)*(1-x)^b/(b*B(a,b)). exp(-800) is below half a subnormal ulp.
    let log_bound = (a - 1.0) * log_x + b * log_complement - b.ln() - log_b;
    if log_bound < -800.0 {
        return Ok(if tail == Tail::Upper { 0.0 } else { 1.0 });
    }
    let log_one_minus_exp = |s: f64| {
        if s > std::f64::consts::LN_2 {
            (-(-s).exp()).ln_1p()
        } else {
            (-(-s).exp_m1()).ln()
        }
    };
    let transformed = b < 1.0;
    let limit = if transformed {
        (b * end.ln()).exp()
    } else {
        end
    };
    let log_density = |u: f64| {
        let s = if transformed { (u.ln() / b).exp() } else { u };
        if transformed {
            let log_ratio = if s < 1e-6 {
                (-s * 0.5 + s * s / 6.0 - s * s * s / 24.0).ln_1p()
            } else {
                log_one_minus_exp(s) - s.ln()
            };
            -a * s + (b - 1.0) * log_ratio - b.ln() - log_b
        } else {
            -a * s + (b - 1.0) * log_one_minus_exp(s) - log_b
        }
    };
    let scale = if transformed {
        -b.ln() - log_b + (b - 1.0) * (log_one_minus_exp(end) - end.ln())
    } else {
        let ratio = (b - 1.0) / a;
        let mode = if ratio.is_finite() {
            ratio.ln_1p()
        } else {
            (b - 1.0).ln() - a.ln()
        };
        log_density(end.min(mode))
    };
    let mut integral = TailIntegral {
        log_density,
        scale,
        remaining: LIMIT,
    };
    let coarse = integral.rule(0.0, limit, &G16, meter)?;
    if coarse <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let value = integral.integrate(0.0, limit, coarse * 2e-13, 20, meter)?;
    if value <= 0.0 {
        return Err(DistributionError::NonConvergence);
    }
    let log = scale + value.ln();
    Ok(if tail == Tail::Upper {
        log.exp()
    } else {
        -log.exp_m1()
    }
    // SAFETY: The clamp bounds are finite ordered literals: 0.0 <= 1.0.
    .clamp(0.0, 1.0))
}

/// Symmetric beta probability offset from its center, retaining small central differences.
/// The log odds density is [x*(1-x)]^a/B(a,a); integrate it without adding one half.
pub(crate) fn beta_center_offset<O>(
    a: f64,
    odds: f64,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    let scale = -2.0 * a * std::f64::consts::LN_2 - log_beta(a, a);
    let mut integral = TailIntegral {
        log_density: |t: f64| {
            let u = t.abs();
            a * (-u - 2.0 * (-u).exp().ln_1p()) - log_beta(a, a)
        },
        scale,
        remaining: LIMIT,
    };
    let value = integral.integrate(0.0, odds.abs(), 1e-15, 20, &mut meter)? * scale.exp();
    Ok(value.copysign(odds))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_cap_depth_and_nonfinite_integrand_have_distinct_failures() {
        let mut exhausted = TailIntegral {
            log_density: |_| 0.0,
            scale: 0.0,
            remaining: 0,
        };
        assert_eq!(
            exhausted.rule(0.0, 1.0, &G8, &mut |_| Ok::<_, ()>(())),
            Err(DistributionError::ComputationalLimit)
        );
        let mut divergent = TailIntegral {
            log_density: |_| f64::NAN,
            scale: 0.0,
            remaining: 100,
        };
        assert_eq!(
            divergent.integrate(0.0, 1.0, 1e-14, 2, &mut |_| Ok::<_, ()>(())),
            Err(DistributionError::NonConvergence)
        );
        let mut unresolved = TailIntegral {
            log_density: |t: f64| t.exp(),
            scale: 0.0,
            remaining: 100,
        };
        assert_eq!(
            unresolved.integrate(0.0, 3.0, 0.0, 0, &mut |_| Ok::<_, ()>(())),
            Err(DistributionError::NonConvergence)
        );
    }
}
