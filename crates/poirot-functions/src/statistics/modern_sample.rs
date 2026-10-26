//! Independently derived, scaled sample statistics; see SAMPLE_SOURCES.md.
use crate::scalar::CellError;
use crate::statistics::modern::{Failure, Result};
use crate::statistics::modern_tails::{self as tails, BetaCoordinate, Tail};

/// Compensated summation of bounded centered terms.
fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0.0, 0.0);
    for x in values {
        let y = x - correction;
        let next = total + y;
        correction = (next - total) - y;
        total = next;
    }
    total
}
/// Scaled, centered moments used to compute sample statistics without forming raw powers.
/// An anchor preserves differences between nearby large observations before normalization.
struct Moments {
    anchor: f64,
    scale: f64,
    mean: f64,
    second: f64,
    third: f64,
    fourth: f64,
    count: f64,
}
impl Moments {
    /// Summarize a finite nonempty admitted sample without raw powers.
    fn new<O>(values: &[f64]) -> std::result::Result<Self, Failure<O>> {
        let first = values
            .first()
            .copied()
            .ok_or(Failure::Spreadsheet(CellError::Div0))?;
        if values.iter().any(|v| !v.is_finite()) {
            return Err(Failure::Spreadsheet(CellError::Num));
        }
        let anchor = if values.iter().all(|v| (v - first).is_finite()) {
            first
        } else {
            0.0
        };
        let scale = values
            .iter()
            .map(|v| (v - anchor).abs())
            .fold(0.0, f64::max);
        let count = values.len() as f64;
        let coordinate = |v: f64| {
            if scale == 0.0 {
                0.0
            } else {
                (v - anchor) / scale
            }
        };
        let mean = sum(values.iter().map(|&v| coordinate(v))) / count;
        let residual = sum(values.iter().map(|&v| coordinate(v) - mean)) / count;
        let mean = mean + residual;
        let central = |power| sum(values.iter().map(|&v| (coordinate(v) - mean).powi(power)));
        Ok(Self {
            anchor,
            scale,
            mean,
            second: central(2),
            third: central(3),
            fourth: central(4),
            count,
        })
    }
    /// Center a member in the admitted bounded coordinate system.
    fn delta(&self, value: f64) -> f64 {
        if self.scale == 0.0 {
            0.0
        } else {
            (value - self.anchor) / self.scale - self.mean
        }
    }
    /// Sample variance represented by its logarithm, without overflowing scale squared.
    fn log_variance(&self) -> f64 {
        2.0 * self.scale.ln() + (self.second / (self.count - 1.0)).ln()
    }
}
fn error<O>(e: CellError) -> Failure<O> {
    Failure::Spreadsheet(e)
}
fn checked<O>(value: f64) -> Result<O> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(error(CellError::Num))
    }
}
/// Standardized sample statistic requested independently of operand admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MomentKind {
    /// Bias-corrected sample skewness.
    SkewSample,
    /// Population skewness, requiring at least three observations.
    SkewPopulation,
    /// Bias-corrected sample excess kurtosis.
    Kurtosis,
}
/// Evaluate dimensionless moments without publishing a potentially overflowing variance.
pub fn moment<O>(values: &[f64], kind: MomentKind) -> Result<O> {
    if values.len() < if kind == MomentKind::Kurtosis { 4 } else { 3 } {
        return Err(error(CellError::Div0));
    }
    let m = Moments::new(values)?;
    if m.second == 0.0 {
        return Err(error(CellError::Div0));
    }
    let n = m.count;
    checked(match kind {
        MomentKind::SkewPopulation => n.sqrt() * m.third / m.second / m.second.sqrt(),
        MomentKind::SkewSample => {
            n * (n - 1.0).sqrt() / (n - 2.0) * m.third / m.second / m.second.sqrt()
        }
        MomentKind::Kurtosis => {
            n * (n + 1.0) * (n - 1.0) / ((n - 2.0) * (n - 3.0)) * (m.fourth / m.second / m.second)
                - 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0))
        }
    })
}
/// Harmonic mean with reciprocals normalized by the smallest positive member.
pub fn harmonic_mean<O>(values: &[f64]) -> Result<O> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite() || *v <= 0.0) {
        return Err(error(CellError::Num));
    }
    let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
    checked(minimum * (values.len() as f64 / sum(values.iter().map(|v| minimum / v))))
}
/// Compact all repeated modes into a prefix in first-occurrence order.
/// Returns that prefix's length; signed zero has one numerical identity.
pub fn modes<O>(
    values: &mut [f64],
    allocation_failure: O,
) -> std::result::Result<usize, Failure<O>> {
    crate::statistics::finite(values).map_err(error)?;
    let key = |v: f64| if v == 0.0 { 0 } else { v.to_bits() };
    let mut counts = std::collections::HashMap::<u64, usize>::new();
    counts
        .try_reserve(values.len())
        .map_err(|_| Failure::Operational(allocation_failure))?;
    for &v in values.iter() {
        // SAFETY: reserve admitted values.len distinct keys; this loop inserts at most that many.
        let count = counts.entry(key(v)).or_default();
        *count = count.checked_add(1).ok_or(error(CellError::Num))?;
    }
    let largest = counts.values().copied().max().unwrap_or(0);
    if largest < 2 {
        return Err(error(CellError::NA));
    }
    let mut written = 0;
    for index in 0..values.len() {
        let value = *values.get(index).ok_or(error(CellError::Num))?;
        if counts.get(&key(value)) == Some(&largest) {
            counts.remove(&key(value));
            *values.get_mut(written).ok_or(error(CellError::Num))? =
                if value == 0.0 { 0.0 } else { value };
            written = written.checked_add(1).ok_or(error(CellError::Num))?;
        }
    }
    Ok(written)
}
/// Exclusive percentile using the one-based position (n+1)k.
pub fn percentile<O>(values: &mut [f64], k: f64) -> Result<O> {
    if values.is_empty() || !k.is_finite() || k <= 0.0 || k >= 1.0 {
        return Err(error(CellError::Num));
    }
    crate::statistics::finite(values).map_err(error)?;
    let h = (values.len() as f64 + 1.0) * k;
    if h < 1.0 || h > values.len() as f64 {
        return Err(error(CellError::Num));
    }
    // SAFETY: total_cmp provides a total order; unstable sorting allocates no storage.
    values.sort_unstable_by(f64::total_cmp);
    let i = (h.floor() as usize)
        .checked_sub(1)
        .ok_or(error(CellError::Num))?;
    let a = *values.get(i).ok_or(error(CellError::Num))?;
    let fraction = h - h.floor();
    if fraction == 0.0 {
        return Ok(a);
    }
    let b = *values
        .get(i.checked_add(1).ok_or(error(CellError::Num))?)
        .ok_or(error(CellError::Num))?;
    // Weighted endpoints avoid an overflowing b-a on opposite extremes.
    checked(if (b - a).is_finite() {
        a + (b - a) * fraction
    } else {
        a * (1.0 - fraction) + b * fraction
    })
}
/// Average rank among exact equal observations; an absent target is unavailable.
pub fn rank_average<O>(values: &[f64], target: f64, ascending: bool) -> Result<O> {
    crate::statistics::finite(values).map_err(error)?;
    if !target.is_finite() {
        return Err(error(CellError::Num));
    }
    let equal = values.iter().filter(|&&v| v == target).count();
    if equal == 0 {
        return Err(error(CellError::NA));
    }
    let before = values
        .iter()
        .filter(|&&v| if ascending { v < target } else { v > target })
        .count();
    Ok(before as f64 + (equal as f64 + 1.0) * 0.5)
}
/// Mean after removing an equal integral count from both sorted tails.
pub fn trimmed_mean<O>(values: &mut [f64], percent: f64) -> Result<O> {
    if !percent.is_finite() || !(0.0..=1.0).contains(&percent) {
        return Err(error(CellError::Num));
    }
    crate::statistics::finite(values).map_err(error)?;
    // SAFETY: total_cmp is a total order; in-place unstable sorting allocates no storage.
    values.sort_unstable_by(f64::total_cmp);
    let remove = (values.len() as f64 * percent * 0.5).floor() as usize;
    let end = values
        .len()
        .checked_sub(remove)
        .ok_or(error(CellError::Num))?;
    let interior = values.get(remove..end).ok_or(error(CellError::Num))?;
    let m = Moments::new(interior)?;
    checked(m.anchor + m.scale * m.mean)
}
/// Fisher transformation or its finite saturating inverse.
pub fn fisher<O>(value: f64, inverse: bool) -> Result<O> {
    if !value.is_finite() || !inverse && value.abs() >= 1.0 {
        return Err(error(CellError::Num));
    }
    checked(if inverse { value.tanh() } else { value.atanh() })
}
/// Permutations of integer-truncated counts, with or without replacement.
pub fn permutations<O>(n: f64, k: f64, replacement: bool) -> Result<O> {
    if !n.is_finite() || !k.is_finite() {
        return Err(error(CellError::Num));
    }
    let (n, k) = (n.trunc(), k.trunc());
    if n < 0.0 || k < 0.0 || !replacement && (n == 0.0 || k > n) {
        return Err(error(CellError::Num));
    }
    if replacement {
        return checked(n.powf(k));
    }
    let (mut result, mut factor) = (1.0, n);
    // More than 1024 factors >=2 necessarily overflows binary64. A possible
    // final factor1 changes nothing; no attacker-sized loop is required.
    for index in 0..=1024 {
        if index as f64 >= k {
            return Ok(result);
        }
        result *= factor;
        if !result.is_finite() {
            return Err(error(CellError::Num));
        }
        factor -= 1.0;
    }
    Err(error(CellError::Num))
}
/// Paired regression result requested after positional admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairKind {
    /// Squared correlation coefficient.
    RSquared,
    /// Standard error of Y about its fitted line on X.
    StandardError,
}
/// Independently scale both coordinates and compute residual squares directly.
pub fn paired<O>(y: &[f64], x: &[f64], kind: PairKind) -> Result<O> {
    if x.len() != y.len() {
        return Err(error(CellError::NA));
    }
    if x.len() < if kind == PairKind::RSquared { 2 } else { 3 } {
        return Err(error(if x.is_empty() && kind == PairKind::RSquared {
            CellError::NA
        } else {
            CellError::Div0
        }));
    }
    let (mx, my) = (Moments::new(x)?, Moments::new(y)?);
    if mx.second == 0.0 || kind == PairKind::RSquared && my.second == 0.0 {
        return Err(error(CellError::Div0));
    }
    let cross = sum(x.iter().zip(y).map(|(&x, &y)| mx.delta(x) * my.delta(y)));
    if kind == PairKind::RSquared {
        return Ok((cross / mx.second.sqrt() / my.second.sqrt())
            .powi(2)
            .min(1.0));
    }
    let slope = cross / mx.second;
    let squares = sum(x
        .iter()
        .zip(y)
        .map(|(&x, &y)| (my.delta(y) - slope * mx.delta(x)).powi(2)));
    checked(my.scale * (squares / (my.count - 2.0)).sqrt())
}
/// Rounded mean and its compensated low component, before comparing groups.
fn mean_parts(sample: &Moments) -> (f64, f64) {
    let offset = sample.scale * sample.mean;
    let product_error = sample.scale.mul_add(sample.mean, -offset);
    let high = sample.anchor + offset;
    // General TwoSum: neither operand needs to dominate the other.
    let offset_part = high - sample.anchor;
    let addition_error = (sample.anchor - (high - offset_part)) + (offset - offset_part);
    (high, addition_error + product_error)
}
/// Recover a product's below-subnormal residual in units of 2^-1022.
fn tiny_mean_correction(sample: &Moments) -> f64 {
    let product = sample.scale * sample.mean;
    if product.abs() >= f64::MIN_POSITIVE || sample.mean == 0.0 {
        return 0.0;
    }
    // A subnormal product with |mean|<=1 can be rescaled before multiplication.
    // If scale were >=4, even the smallest nonzero mean would give >=4 ulps,
    // so scaling by 2^1022 cannot overflow for an underflowed product.
    // For a nonzero subnormal product scale may be larger; split its exponent
    // across both factors rather than overflowing the larger one.
    let scaled_product = if sample.scale <= 1.0 {
        libm::scalbn(sample.scale, 1022) * sample.mean
    } else {
        sample.scale * libm::scalbn(sample.mean, 1022)
    };
    let rounded_error = sample.scale.mul_add(sample.mean, -product);
    scaled_product - libm::scalbn(product, 1022) - libm::scalbn(rounded_error, 1022)
}
/// Signed/log mean difference, scaling actual mean components rather than spread.
fn mean_difference(a: &Moments, b: &Moments) -> (f64, f64) {
    let (a_high, a_low) = mean_parts(a);
    let (b_high, b_low) = mean_parts(b);
    let high = a_high - b_high;
    let low = a_low - b_low;
    if high.is_infinite() {
        let half_high = a_high * 0.5 - b_high * 0.5;
        let half_low = a_low * 0.5 - b_low * 0.5;
        let scale = half_high.abs().max(half_low.abs());
        let delta = half_high / scale + half_low / scale;
        return (
            delta,
            scale.ln() + delta.abs().ln() + std::f64::consts::LN_2,
        );
    }
    let scale = high.abs().max(low.abs());
    if scale < 4.0 * f64::MIN_POSITIVE {
        let delta = libm::scalbn(high, 1022) + libm::scalbn(low, 1022) + tiny_mean_correction(a)
            - tiny_mean_correction(b);
        return (delta, delta.abs().ln() - 1022.0 * std::f64::consts::LN_2);
    }
    let delta = high / scale + low / scale;
    (delta, scale.ln() + delta.abs().ln())
}
/// Log-sum-exp of two nonnegative quantities given their logarithms.
fn log_add(a: f64, b: f64) -> f64 {
    let high = a.max(b);
    if high == f64::NEG_INFINITY {
        high
    } else {
        high + (a.min(b) - high).exp().ln_1p()
    }
}
/// Return a Student two-tail probability retaining fractional degrees and tiny coordinates.
fn t_probability<O>(
    log_t: f64,
    degrees: f64,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    if log_t == f64::NEG_INFINITY {
        return Ok(1.0);
    }
    let ratio = 2.0 * log_t - degrees.ln();
    let coordinate = if ratio > 0.0 {
        BetaCoordinate::LowerLog(-ratio - (-ratio).exp().ln_1p())
    } else {
        BetaCoordinate::UpperLog(ratio - ratio.exp().ln_1p())
    };
    tails::beta(degrees * 0.5, 0.5, coordinate, Tail::Lower, meter)
}
/// Kind of Student comparison, separate from one/two-tail publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestKind {
    /// Matched differences, supplied after aligned pair admission.
    Paired,
    /// Independent samples with a pooled common variance.
    Pooled,
    /// Independent samples with unequal variances and fractional Welch degrees.
    Welch,
}
/// Student comparison on admitted arrays with bounded metered distribution tails.
pub fn t_test<O>(
    x: &[f64],
    y: &[f64],
    kind: TestKind,
    two_tails: bool,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    if kind == TestKind::Paired && x.len() != y.len() {
        return Err(error(CellError::NA));
    }
    if x.len() < 2 || y.len() < 2 {
        return Err(error(CellError::Div0));
    }
    let (log_t, degrees) = if kind == TestKind::Paired {
        meter(x.len() as u64).map_err(Failure::Operational)?;
        let half = x.iter().zip(y).any(|(&a, &b)| !(a - b).is_finite());
        let factor = if half { 0.5 } else { 1.0 };
        let mut differences = Vec::new();
        differences
            .try_reserve_exact(x.len())
            .map_err(|_| Failure::ComputationalLimit)?;
        for (&a, &b) in x.iter().zip(y) {
            // SAFETY: x.len slots were reserved and zip emits at most x.len pairs.
            differences.push(a * factor - b * factor);
        }
        let m = Moments::new(&differences)?;
        if m.second == 0.0 {
            return Err(error(CellError::Div0));
        }
        let zero = Moments::new(&[0.0])?;
        (
            mean_difference(&m, &zero).1 - 0.5 * (m.log_variance() - m.count.ln()),
            m.count - 1.0,
        )
    } else {
        let (a, b) = (Moments::new(x)?, Moments::new(y)?);
        let (va, vb) = (a.log_variance(), b.log_variance());
        let (log_se2, degrees) = if kind == TestKind::Pooled {
            let df = a.count + b.count - 2.0;
            (
                log_add(va + (a.count - 1.0).ln(), vb + (b.count - 1.0).ln()) - df.ln()
                    + (1.0 / a.count + 1.0 / b.count).ln(),
                df,
            )
        } else {
            let (u, v) = (va - a.count.ln(), vb - b.count.ln());
            let se = log_add(u, v);
            let weight = (u - se).exp();
            (
                se,
                1.0 / (weight * weight / (a.count - 1.0)
                    + (1.0 - weight).powi(2) / (b.count - 1.0)),
            )
        };
        if log_se2 == f64::NEG_INFINITY {
            return Err(error(CellError::Div0));
        }
        (mean_difference(&a, &b).1 - 0.5 * log_se2, degrees)
    };
    let probability = t_probability(log_t, degrees, &mut meter)?;
    Ok(if two_tails {
        probability
    } else {
        probability * 0.5
    })
}
/// Two-sided comparison of nonzero sample variances using both beta tails.
pub fn f_test<O>(
    x: &[f64],
    y: &[f64],
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    if x.len() < 2 || y.len() < 2 {
        return Err(error(CellError::Div0));
    }
    let (a, b) = (Moments::new(x)?, Moments::new(y)?);
    if a.second == 0.0 || b.second == 0.0 {
        return Err(error(CellError::Div0));
    }
    let ratio = a.log_variance() - b.log_variance() + (a.count - 1.0).ln() - (b.count - 1.0).ln();
    let coordinate = if ratio > 0.0 {
        BetaCoordinate::UpperLog(-ratio - (-ratio).exp().ln_1p())
    } else {
        BetaCoordinate::LowerLog(ratio - ratio.exp().ln_1p())
    };
    let lower = tails::beta(
        (a.count - 1.0) * 0.5,
        (b.count - 1.0) * 0.5,
        coordinate,
        Tail::Lower,
        &mut meter,
    )?;
    let upper = tails::beta(
        (a.count - 1.0) * 0.5,
        (b.count - 1.0) * 0.5,
        coordinate,
        Tail::Upper,
        meter,
    )?;
    Ok((2.0 * lower.min(upper)).min(1.0))
}
/// Chi-square goodness-of-fit retaining the supplied rectangular dimensions.
pub fn chi_square_test<O>(
    observed: &[f64],
    expected: &[f64],
    rows: usize,
    columns: usize,
    meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<O> {
    if observed.len() != expected.len() || rows.checked_mul(columns) != Some(observed.len()) {
        return Err(error(CellError::NA));
    }
    let degrees = if rows > 1 && columns > 1 {
        rows.checked_sub(1)
            .and_then(|r| columns.checked_sub(1).and_then(|c| r.checked_mul(c)))
            .ok_or(error(CellError::Num))?
    } else {
        observed.len().saturating_sub(1)
    };
    if degrees == 0 {
        return Err(error(CellError::NA));
    }
    crate::statistics::finite(observed).map_err(error)?;
    crate::statistics::finite(expected).map_err(error)?;
    if observed.iter().any(|v| *v < 0.0) || expected.iter().any(|v| *v < 0.0) {
        return Err(error(CellError::Num));
    }
    if expected.contains(&0.0) {
        return Err(error(CellError::Div0));
    }
    let (mut statistic, mut correction) = (0.0, 0.0);
    for (&observed, &expected) in observed.iter().zip(expected) {
        let standardized = (observed - expected).abs() / expected.sqrt();
        let term = standardized * standardized;
        // Every term is nonnegative. An overflowing term or partial sum
        // exceeds f64::MAX. The checked rows*columns equals observed.len(),
        // so this function's degrees <= observed.len() <= usize::MAX; the gamma
        // argument then dominates its shape by over 10^280. Its upper tail
        // is below the smallest binary64 number, so publication is zero.
        // Detect it before compensation could turn infinity-infinity into NaN.
        if !term.is_finite() || !(statistic + term).is_finite() {
            return Ok(0.0);
        }
        let adjusted = term - correction;
        let next = statistic + adjusted;
        if !next.is_finite() {
            return Ok(0.0);
        }
        correction = (next - statistic) - adjusted;
        statistic = next;
    }
    tails::gamma(degrees as f64 * 0.5, statistic * 0.5, Tail::Upper, meter)
}
/// Signed normal survival probability using an optional known population deviation.
pub fn z_test<O>(values: &[f64], hypothesis: f64, sigma: Option<f64>) -> Result<O> {
    if values.is_empty() {
        return Err(error(CellError::NA));
    }
    if !hypothesis.is_finite() {
        return Err(error(CellError::Num));
    }
    let m = Moments::new(values)?;
    let log_se = if let Some(sigma) = sigma {
        if !sigma.is_finite() || sigma < 0.0 {
            return Err(error(CellError::Num));
        }
        if sigma == 0.0 {
            return Err(error(CellError::Div0));
        }
        sigma.ln() - 0.5 * m.count.ln()
    } else {
        if values.len() < 2 || m.second == 0.0 {
            return Err(error(CellError::Div0));
        }
        0.5 * (m.log_variance() - m.count.ln())
    };
    let reference = Moments::new(&[hypothesis])?;
    let (signed, log_delta) = mean_difference(&m, &reference);
    let z = (log_delta - log_se).exp().copysign(signed);
    Ok(0.5 * libm::erfc(z * std::f64::consts::FRAC_1_SQRT_2))
}
/// Probability mass in inclusive bounds, preserving exact positional associations.
pub fn probability<O>(x: &[f64], p: &[f64], lower: f64, upper: f64) -> Result<O> {
    if x.len() != p.len() {
        return Err(error(CellError::NA));
    }
    if x.is_empty()
        || !lower.is_finite()
        || !upper.is_finite()
        || x.iter().any(|v| !v.is_finite())
        || p.iter().any(|v| !v.is_finite() || *v <= 0.0 || *v > 1.0)
        || sum(p.iter().copied()) != 1.0
    {
        return Err(error(CellError::Num));
    }
    Ok(sum(x.iter().zip(p).filter_map(|(&x, &p)| {
        (x >= lower && x <= upper).then_some(p)
    })))
}
#[cfg(test)]
#[path = "modern_sample_tests.rs"]
mod tests;
