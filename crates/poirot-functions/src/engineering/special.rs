//! Integer-order Bessel, Gaussian integral and numeric threshold kernels.
use super::bessel_k::{log_scaled_k0, log_scaled_k1};

/// The real integer-order cylindrical solution to evaluate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bessel {
    /// Ordinary first-kind solution J.
    First,
    /// Ordinary second-kind solution Y, on positive arguments.
    Second,
    /// Modified first-kind solution I.
    ModifiedFirst,
    /// Modified second-kind solution K, on positive arguments.
    ModifiedSecond,
}
/// Numerical admission or execution failure, independent of worksheet errors.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure<O> {
    /// The argument is outside the finite real domain or the answer overflows.
    Number,
    /// The caller's execution budget refused work.
    Operational(O),
    /// The requested order or series exceeds the explicitly bounded kernel.
    ComputationalLimit,
}
/// Evaluate a real integer-order solution, truncating only nonnegative orders.
///
/// Orders above 100,000 or modified-I arguments above 100,000 return an operational
/// computational limit. The caller meters iteration work before it executes.
pub fn bessel<O>(
    kind: Bessel,
    x: f64,
    order: f64,
    mut meter: impl FnMut(u64) -> Result<(), O>,
) -> Result<f64, Failure<O>> {
    if !x.is_finite() || !order.is_finite() || order < 0.0 {
        return Err(Failure::Number);
    }
    if matches!(kind, Bessel::Second | Bessel::ModifiedSecond) && x <= 0.0 {
        return Err(Failure::Number);
    }
    if order.trunc() > 100_000.0 {
        return Err(Failure::ComputationalLimit);
    }
    let n = order.trunc() as u32;
    meter(1).map_err(Failure::Operational)?;
    let answer = match kind {
        Bessel::First | Bessel::Second => {
            // The admitted integer fits i32. libm's bounded-order recurrence
            // and its fixed base approximations are charged before execution.
            meter(u64::from(n) + 10_001).map_err(Failure::Operational)?;
            if kind == Bessel::First {
                libm::jn(n as i32, x)
            } else {
                libm::yn(n as i32, x)
            }
        }
        Bessel::ModifiedFirst => {
            if x == 0.0 {
                return Ok(if n == 0 { 1.0 } else { 0.0 });
            }
            let answer = modified_i(x.abs(), n, &mut meter)?;
            if x < 0.0 && n % 2 == 1 {
                -answer
            } else {
                answer
            }
        }
        Bessel::ModifiedSecond => {
            let base0 = log_scaled_k0(x);
            if n == 0 {
                return finite((base0 - x).exp());
            }
            let base1 = log_scaled_k1(x);
            if n == 1 {
                return finite((base1 - x).exp());
            }
            let initial = base0.max(base1);
            let mut previous = (base0 - initial).exp();
            let mut current = (base1 - initial).exp();
            let mut exponent = 0_u32;
            for k in 1..n {
                meter(1).map_err(Failure::Operational)?;
                // DLMF10.29: K_(k+1)=K_(k-1)+(2k/x)K_k.
                // Binary scaling avoids logarithmic roundoff on every step;
                // base values never underflow before the order can recover them.
                const SCALE: f64 = 1.157920892373162e77; // 2^256
                if current > SCALE {
                    previous /= SCALE;
                    current /= SCALE;
                    exponent = exponent
                        .checked_add(256)
                        .ok_or(Failure::ComputationalLimit)?;
                }
                let coefficient = 2.0 * f64::from(k) / x;
                // K increases with nonnegative order (positive cosh integral).
                // Thus an already overflowing positive summand proves final
                // overflow; this also protects tiny-x multiplication below.
                let log_product = current.ln()
                    + coefficient.ln()
                    + initial
                    + f64::from(exponent) * std::f64::consts::LN_2
                    - x;
                if log_product > f64::MAX.ln() {
                    return Err(Failure::Number);
                }
                let next = previous + coefficient * current;
                previous = current;
                current = next;
            }
            (current.ln() + f64::from(exponent).mul_add(std::f64::consts::LN_2, initial - x)).exp()
        }
    };
    finite(answer)
}
fn finite<O>(value: f64) -> Result<f64, Failure<O>> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Failure::Number)
    }
}
fn modified_i<O>(
    x: f64,
    n: u32,
    meter: &mut impl FnMut(u64) -> Result<(), O>,
) -> Result<f64, Failure<O>> {
    if x > 100_000.0 {
        return Err(Failure::ComputationalLimit);
    }
    let v = f64::from(n);
    // DLMF10.25.2, positive series. Start at its largest term, not the
    // potentially underflowing k=0 term. Both tails then decrease monotonically.
    let mode = (x * x / (2.0 * ((v * v + x * x).sqrt() + v))).floor() as u32;
    let k = f64::from(mode);
    let log_seed = log_series_weight(k, x) + log_series_weight(v + k, x);
    if log_seed > f64::MAX.ln() {
        return Err(Failure::Number);
    }
    let mut sum = 1.0;
    let mut term = 1.0;
    for j in (1..=mode).rev() {
        meter(1).map_err(Failure::Operational)?;
        let j = f64::from(j);
        term *= (2.0 * j / x) * (2.0 * (v + j) / x);
        sum += term;
        if term <= sum * f64::EPSILON / 8.0 {
            break;
        }
    }
    term = 1.0;
    // SAFETY: x<=100000 and n>=0 imply mode<=x/2<=50000, so mode+1 fits u32.
    for j in mode + 1..=100_001 {
        meter(1).map_err(Failure::Operational)?;
        let j = f64::from(j);
        term *= (x / (2.0 * j)) * (x / (2.0 * (v + j)));
        sum += term;
        // The ratio decreases thereafter, so geometric tail bounds apply.
        let ratio = (x / (2.0 * (j + 1.0))) * (x / (2.0 * (v + j + 1.0)));
        if ratio < 1.0 && term * ratio / (1.0 - ratio) <= sum * f64::EPSILON / 8.0 {
            return finite((log_seed + sum.ln()).exp());
        }
    }
    Err(Failure::ComputationalLimit)
}
// Log((x/2)^count/count!), rearranged before subtracting large factorials.
// DLMF5.11.1 Stirling series; for count>=32 the first omitted term is
// bounded by691/(360360*count^11), below6e-20. Small factorials use libm.
fn log_series_weight(count: f64, x: f64) -> f64 {
    if count == 0.0 {
        return 0.0;
    }
    if count < 32.0 {
        return count * (x.ln() - std::f64::consts::LN_2) - libm::lgamma(count + 1.0);
    }
    let inverse = 1.0 / count;
    let square = inverse * inverse;
    let correction = inverse
        * (1.0 / 12.0
            + square
                * (-1.0 / 360.0
                    + square * (1.0 / 1260.0 + square * (-1.0 / 1680.0 + square / 1188.0))));
    count * (x / (2.0 * count)).ln() + count - 0.5 * count.ln() - 0.9189385332046727 - correction
}
/// Return the Gaussian integral from zero to x, using the existing libm backend.
pub fn erf(x: f64) -> f64 {
    libm::erf(x)
}
/// Return the complementary Gaussian tail directly, without subtracting from one.
pub fn erfc(x: f64) -> f64 {
    libm::erfc(x)
}
/// Return the directed Gaussian integral between two finite endpoints.
pub fn erf_interval(a: f64, b: f64) -> f64 {
    if a == b {
        return 0.0;
    }
    if a > b {
        return -erf_interval(b, a);
    }
    // A short interval admits an independent local Taylor integral. Expanding
    // exp(-2*a*t-t*t) avoids cancellation of two nearly equal tail values.
    let width = b - a;
    if width.is_finite() && width * (1.0 + a.abs().max(b.abs())) < 0.01 {
        let middle = a + width / 2.0;
        let half = width / 2.0;
        // Midpoint Gaussian expansion through h^6; the admitted |m*h| <.005
        // makes the omitted even h^8 terms smaller than binary64 precision.
        let mh = middle * half;
        let h2 = half * half;
        let m2 = mh * mh;
        let correction = 1.0
            + (2.0 * m2 - h2) / 3.0
            + (4.0 * m2 * m2 - 12.0 * m2 * h2 + 3.0 * h2 * h2) / 30.0
            + (8.0 * m2 * m2 * m2 - 60.0 * m2 * m2 * h2 + 90.0 * m2 * h2 * h2
                - 15.0 * h2 * h2 * h2)
                / 630.0;
        return std::f64::consts::FRAC_2_SQRT_PI * (-middle * middle).exp() * width * correction;
    }
    if a >= 0.0 {
        erfc(a) - erfc(b)
    } else if b <= 0.0 {
        erfc(-b) - erfc(-a)
    } else {
        erf(b) - erf(a)
    }
}
/// Compare a number with a threshold, including equality of signed zeros.
pub fn gestep(number: f64, threshold: f64) -> f64 {
    if number >= threshold { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn compute(kind: Bessel, x: f64, n: f64) -> f64 {
        bessel(kind, x, n, |_| Ok::<_, ()>(())).unwrap()
    }
    fn relative(actual: f64, expected: f64, tolerance: f64) {
        assert!(actual.is_finite());
        assert!(
            (actual / expected - 1.0).abs() <= tolerance,
            "actual={actual:e}, expected={expected:e}, relative={:e}",
            (actual / expected - 1.0).abs()
        );
    }
    #[test]
    fn independent_positive_series_and_integral_oracles() {
        // Independently generated at90 decimal digits from DLMF10.25.2's
        // positive I series and10.32.9's K integral, not another Bessel routine.
        // The test cases/derivation are Poirot-authored; no MPL fixtures reused.
        for (kind, n, x, expected) in [
            (Bessel::ModifiedFirst, 0.0, 1e-300, 1.0),
            (Bessel::ModifiedFirst, 1.0, 1e-300, 5e-301),
            (Bessel::ModifiedFirst, 0.0, 0.5, 1.0634833707413236),
            (Bessel::ModifiedFirst, 1.0, 1.0, 0.565159103992485),
            (Bessel::ModifiedFirst, 2.0, 1.9, 0.6032724329434784),
            (Bessel::ModifiedFirst, 10.0, 10.0, 21.89170616372337),
            (Bessel::ModifiedFirst, 50.0, 50.0, 17650802430.016712),
            (Bessel::ModifiedFirst, 100.0, 200.0, 4.352750449727022e+74),
            (Bessel::ModifiedFirst, 500.0, 800.0, 4.765569742883583e+279),
            (
                Bessel::ModifiedFirst,
                1000.0,
                1000.0,
                2.723453646910843e+229,
            ),
            (Bessel::ModifiedFirst, 100.0, 1.0, 8.47367400813808e-189),
            (Bessel::ModifiedSecond, 0.0, 1e-300, 690.8914594138721),
            (Bessel::ModifiedSecond, 0.0, 0.999999, 0.42102504014845),
            (Bessel::ModifiedSecond, 1.0, 1.0, 0.6019072301972346),
            (Bessel::ModifiedSecond, 0.0, 1.000001, 0.4210238363339896),
            (Bessel::ModifiedSecond, 1.0, 1.000001, 0.6019062072666795),
            (Bessel::ModifiedSecond, 2.0, 2.3, 0.16173336243284375),
            (Bessel::ModifiedSecond, 10.0, 10.0, 0.00161425530039067),
            (Bessel::ModifiedSecond, 50.0, 50.0, 4.0060134766400893e-13),
            (Bessel::ModifiedSecond, 100.0, 200.0, 5.1371380077224475e-78),
            (
                Bessel::ModifiedSecond,
                500.0,
                800.0,
                1.1121417730640808e-283,
            ),
            (
                Bessel::ModifiedSecond,
                1000.0,
                1000.0,
                1.298180251466701e-233,
            ),
            (Bessel::ModifiedSecond, 0.0, 700.0, 4.669776431685377e-306),
            (
                Bessel::ModifiedFirst,
                2000.0,
                1000.0,
                1.2950953907756806e-285,
            ),
            (
                Bessel::ModifiedFirst,
                100000.0,
                66000.0,
                1.1950577601191333e-219,
            ),
            (
                Bessel::ModifiedSecond,
                100000.0,
                66000.0,
                3.491920785791348e213,
            ),
            (
                Bessel::ModifiedFirst,
                10000.0,
                6600.0,
                9.188078575691668e-25,
            ),
            (Bessel::ModifiedSecond, 10000.0, 6600.0, 4.54180598880772e19),
            (
                Bessel::ModifiedSecond,
                2000.0,
                1000.0,
                1.726_566_212_716_972e281,
            ),
        ] {
            let actual = compute(kind, x, n);
            let difference = (actual / expected - 1.0).abs();
            eprintln!("{kind:?} n={n} x={x:e} relative={difference:e}");
            let tolerance = if n <= 50.0 {
                3e-13
            } else if n <= 2000.0 {
                2e-12
            } else {
                1e-10
            };
            relative(actual, expected, tolerance);
        }
    }
    #[test]
    fn exact_symmetry_domains_overflow_and_bounded_work() {
        for kind in [Bessel::First, Bessel::ModifiedFirst] {
            assert_eq!(compute(kind, 0.0, 0.0), 1.0);
            assert_eq!(compute(kind, 0.0, 3.0), 0.0);
            relative(compute(kind, -2.0, 3.9), -compute(kind, 2.0, 3.0), 1e-14);
            assert_eq!(
                bessel(kind, 1.0, -0.1, |_| Ok::<_, ()>(())),
                Err(Failure::Number)
            );
        }
        for kind in [Bessel::Second, Bessel::ModifiedSecond] {
            for x in [0.0, -1.0] {
                assert_eq!(
                    bessel(kind, x, 0.0, |_| Ok::<_, ()>(())),
                    Err(Failure::Number)
                );
            }
        }
        assert_eq!(
            bessel(Bessel::ModifiedFirst, 1000.0, 0.0, |_| Ok::<_, ()>(())),
            Err(Failure::Number)
        );
        assert_eq!(
            bessel(Bessel::ModifiedSecond, 1e-300, 2.0, |_| Ok::<_, ()>(())),
            Err(Failure::Number)
        );
        assert_eq!(compute(Bessel::ModifiedSecond, 1e300, 10.0), 0.0);
        assert_eq!(
            bessel(Bessel::ModifiedFirst, 1.0, 100001.0, |_| Ok::<_, ()>(())),
            Err(Failure::ComputationalLimit)
        );
        assert_eq!(
            bessel(Bessel::ModifiedFirst, 100001.0, 1.0, |_| Ok::<_, ()>(())),
            Err(Failure::ComputationalLimit)
        );
        for kind in [
            Bessel::First,
            Bessel::Second,
            Bessel::ModifiedFirst,
            Bessel::ModifiedSecond,
        ] {
            assert_eq!(
                bessel(kind, 10.0, 10.0, |_| Err("work")),
                Err(Failure::Operational("work"))
            );
        }
    }
    #[test]
    fn gaussian_tails_intervals_and_thresholds_do_not_cancel() {
        assert_eq!(erf(0.0), 0.0);
        assert_eq!(erfc(0.0), 1.0);
        relative(erfc(8.0), 1.1224297172982926e-29, 2e-14);
        relative(erf_interval(6.0, 8.0), 2.151973671248769e-17, 2e-14);
        let a = 8.0_f64;
        let b = f64::from_bits(a.to_bits() + 1);
        let width = b - a;
        relative(
            erf_interval(a, b),
            std::f64::consts::FRAC_2_SQRT_PI * (-a * a).exp() * width,
            3e-14,
        );
        assert_eq!(erf_interval(b, a), -erf_interval(a, b));
        assert_eq!(erf_interval(a, a), 0.0);
        relative(erf_interval(-8.0, -6.0), erf_interval(6.0, 8.0), 1e-15);
        assert_eq!(gestep(-0.0, 0.0), 1.0);
        assert_eq!(gestep(2.0, 3.0), 0.0);
    }
}
