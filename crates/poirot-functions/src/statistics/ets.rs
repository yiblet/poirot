//! Bounded additive innovations forecasting over a shared regularized timeline.
//! Numerical and native-compatibility policies are recorded in ETS_SOURCES.md.
use crate::coercion::{date_time::CalendarEpoch, dates::calendar::civil};
use crate::scalar::CellError;
use crate::statistics::modern::Failure;

/// Kernel value with spreadsheet and caller execution failures kept separate.
pub type Result<T, O> = std::result::Result<T, Failure<O>>;
/// Duplicate timestamp reduction from the documented XlForecastAggregation enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aggregation {
    /// Mean of numeric observations.
    Average,
    /// Number of numeric observations.
    Count,
    /// Number of admitted nonempty observations.
    CountA,
    /// Largest observation.
    Maximum,
    /// Middle observation, averaging the central pair.
    Median,
    /// Smallest observation.
    Minimum,
    /// Sum of observations.
    Sum,
}
impl Aggregation {
    /// Decode the explicit first-party 1..7 enum, independently of omission defaults.
    pub fn from_number(value: f64) -> std::result::Result<Self, CellError> {
        match value {
            1.0 => Ok(Self::Average),
            2.0 => Ok(Self::Count),
            3.0 => Ok(Self::CountA),
            4.0 => Ok(Self::Maximum),
            5.0 => Ok(Self::Median),
            6.0 => Ok(Self::Minimum),
            7.0 => Ok(Self::Sum),
            _ => Err(CellError::Num),
        }
    }
}
/// How interior missing timestamps are completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    /// Supply zero at each missing timestamp.
    Zero,
    /// Interpolate linearly between the two surrounding observations.
    Interpolate,
}
/// Seasonal period selection, separate from the fitted smoothing gains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seasonality {
    /// Fit additive level and trend without a seasonal state.
    None,
    /// Detect a repeated pattern after removing a fitted straight trend.
    Automatic,
    /// Fit a caller-specified seasonal period.
    Period(usize),
}
impl Seasonality {
    /// Admit Excel's nonseasonal, automatic, or explicit whole-period selectors.
    pub fn from_number(value: f64) -> std::result::Result<Self, CellError> {
        if !value.is_finite() || value < 0.0 || value > 8760.0 || value != value.trunc() {
            return Err(CellError::Num);
        }
        Ok(if value == 0.0 {
            Self::None
        } else if value == 1.0 {
            Self::Automatic
        } else {
            Self::Period(value as usize)
        })
    }
}
/// Numeric or calendar-month coordinate system used by a prepared timeline.
#[derive(Clone, Copy, Debug)]
enum Grid {
    /// Uniform numeric spacing, including daily date serials.
    Numeric,
    /// Same-day calendar months, interpreted in the workbook epoch.
    Months { epoch: CalendarEpoch, day: i64 },
}
/// A regularized time series ready for ETS fitting and future-timestamp projection.
/// It retains numeric or calendar-month spacing so forecast horizons use the same grid as the observations.
#[derive(Debug)]
pub struct Series {
    values: Vec<f64>,
    end: f64,
    step: f64,
    grid: Grid,
}
fn num<O>() -> Failure<O> {
    Failure::Spreadsheet(CellError::Num)
}
fn allocation<O>() -> Failure<O> {
    Failure::ComputationalLimit
}
/// Reserve before writing any kernel-owned sample/state storage.
fn buffer<T, O>(len: usize) -> Result<Vec<T>, O> {
    let mut result = Vec::new();
    result.try_reserve_exact(len).map_err(|_| allocation())?;
    Ok(result)
}
/// Resolve an integral supported date serial into a calendar month and day.
fn month(value: f64, epoch: CalendarEpoch) -> Option<(f64, i64)> {
    if !value.is_finite() || value != value.trunc() || !(1.0..=2_957_003.0).contains(&value) {
        return None;
    }
    let serial = value as i64;
    if matches!(epoch, CalendarEpoch::Excel1900) && serial == 60 {
        return None;
    }
    // SAFETY: serial was bounded to 1..=2957003; the two fixed epoch subtractions
    // and possible one-day correction stay inside i64. civil returns these dates.
    let date = civil(match epoch {
        CalendarEpoch::Excel1900 => serial - 25568 - i64::from(serial > 60),
        CalendarEpoch::Excel1904 => serial - 24107,
    });
    let index = date.y.checked_mul(12)?.checked_add(date.m)?;
    Some((index as f64, date.d))
}
/// Return a constant step when every observed gap is an integral multiple of it.
fn step(points: &[(f64, f64)]) -> Option<f64> {
    let mut smallest = f64::INFINITY;
    for pair in points.windows(2) {
        let [a, b] = pair else { continue };
        smallest = smallest.min(b.0 - a.0);
    }
    if !smallest.is_finite() || smallest <= 0.0 {
        return None;
    }
    let first = points.first()?.0;
    if points.iter().all(|p| {
        let position = (p.0 - first) / smallest;
        position.is_finite() && (position - position.round()).abs() <= 1e-8
    }) {
        Some(smallest)
    } else {
        None
    }
}
/// Sort aligned observations, reduce duplicates, and complete a bounded regular grid.
/// At most 30 percent of the final grid may be missing. Native boundary and
/// origin admission ambiguities are declared in the policy ledger.
pub fn prepare<O>(
    mut points: Vec<(f64, f64)>,
    aggregation: Aggregation,
    completion: Completion,
    epoch: CalendarEpoch,
    max_values: usize,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<Series, O> {
    if points.len() < 2
        || points
            .iter()
            .any(|(t, y)| !t.is_finite() || *t == 0.0 || !y.is_finite())
    {
        return Err(num());
    }
    meter(
        (points.len() as u64)
            .saturating_mul(8 + u64::from(usize::BITS - points.len().leading_zeros())),
    )
    .map_err(Failure::Operational)?;
    points.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut reduced = buffer(points.len())?;
    let mut start = 0;
    while start < points.len() {
        let timestamp = points.get(start).ok_or_else(num)?.0;
        let mut end = start;
        while points.get(end).is_some_and(|p| p.0 == timestamp) {
            end = end.checked_add(1).ok_or_else(allocation)?;
        }
        let group = points.get_mut(start..end).ok_or_else(num)?;
        let n = group.len() as f64;
        let value = match aggregation {
            Aggregation::Count | Aggregation::CountA => n,
            Aggregation::Average => group.iter().map(|p| p.1 / n).sum(),
            Aggregation::Sum => group.iter().map(|p| p.1).sum(),
            Aggregation::Minimum => group.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
            Aggregation::Maximum => group.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
            Aggregation::Median => {
                group.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
                let right = group.get(group.len() / 2).ok_or_else(num)?.1;
                if group.len() % 2 == 1 {
                    right
                } else {
                    let left_index = (group.len() / 2).checked_sub(1).ok_or_else(num)?;
                    let left = group.get(left_index).ok_or_else(num)?.1;
                    left * 0.5 + right * 0.5
                }
            }
        };
        if !value.is_finite() {
            return Err(num());
        }
        // SAFETY: points.len slots reserved and every group contains at least one point.
        reduced.push((timestamp, value));
        start = end;
    }
    let mut grid = Grid::Numeric;
    let numeric_step = step(&reduced);
    let calendar = reduced
        .first()
        .and_then(|p| month(p.0, epoch))
        .filter(|(_, day)| {
            reduced
                .iter()
                .all(|p| month(p.0, epoch).is_some_and(|(_, d)| d == *day))
        })
        .filter(|_| {
            reduced.windows(2).all(|pair| match pair {
                [a, b] => b.0 - a.0 >= 28.0,
                _ => false,
            })
        });
    let spacing = if let Some(spacing) = numeric_step.filter(|_| calendar.is_none()) {
        spacing
    } else {
        let (_, day) = month(reduced.first().ok_or_else(num)?.0, epoch).ok_or_else(num)?;
        for point in &mut reduced {
            let (index, d) = month(point.0, epoch).ok_or_else(num)?;
            if d != day {
                return Err(num());
            }
            point.0 = index;
        }
        grid = Grid::Months { epoch, day };
        step(&reduced).ok_or_else(num)?
    };
    let first = reduced.first().ok_or_else(num)?.0;
    let end = reduced.last().ok_or_else(num)?.0;
    let intervals = ((end - first) / spacing).round();
    if !intervals.is_finite() || intervals < 1.0 {
        return Err(num());
    }
    // Validate data before the allocation envelope: a very sparse invalid grid
    // is a worksheet-domain error, not an operational capacity refusal.
    let grid_length = intervals + 1.0;
    let missing = grid_length - reduced.len() as f64;
    if missing < 0.0 || missing / grid_length > 0.3 {
        return Err(num());
    }
    if intervals >= max_values as f64 {
        return Err(allocation());
    }
    let length = (intervals as usize).checked_add(1).ok_or_else(allocation)?;
    meter((length as u64).saturating_mul(8)).map_err(Failure::Operational)?;
    let mut values = buffer(length)?;
    let mut right = 1;
    for i in 0..length {
        let coordinate = first + spacing * i as f64;
        while reduced
            .get(right)
            .is_some_and(|p| p.0 < coordinate - spacing * 1e-8)
        {
            right = right.checked_add(1).ok_or_else(allocation)?;
        }
        let previous = reduced.get(right.saturating_sub(1)).ok_or_else(num)?;
        let next = reduced.get(right).unwrap_or(previous);
        let value = if (coordinate - previous.0).abs() <= spacing * 1e-8 {
            previous.1
        } else if (coordinate - next.0).abs() <= spacing * 1e-8 {
            next.1
        } else if completion == Completion::Zero {
            0.0
        } else {
            let fraction = (coordinate - previous.0) / (next.0 - previous.0);
            (1.0 - fraction) * previous.1 + fraction * next.1
        };
        if !value.is_finite() {
            return Err(num());
        }
        // SAFETY: length slots reserved and the loop writes exactly length values.
        values.push(value);
    }
    Ok(Series {
        values,
        end,
        step: spacing,
        grid,
    })
}
impl Series {
    /// Borrow the regularized observations, including interior completions.
    pub fn values(&self) -> &[f64] {
        &self.values
    }
    /// Return spacing in numeric units, or months for a calendar grid.
    pub fn step(&self) -> f64 {
        self.step
    }
    /// Convert a supported future timestamp to its nonnegative grid horizon.
    /// Fractional numeric horizons interpolate the adjacent model projections.
    pub fn horizon(&self, target: f64) -> std::result::Result<f64, CellError> {
        let coordinate = match self.grid {
            Grid::Numeric => target,
            Grid::Months { epoch, day } => {
                let (index, d) = month(target, epoch).ok_or(CellError::Num)?;
                if d != day {
                    return Err(CellError::Num);
                }
                index
            }
        };
        let horizon = (coordinate - self.end) / self.step;
        if horizon.is_finite() && horizon >= 0.0 {
            Ok(horizon)
        } else {
            Err(CellError::Num)
        }
    }
}
/// Weights that apply each one-step forecast error to the ETS level, trend, and seasonal state.
/// These are direct innovations gains; beta and gamma are not Holt's external weights.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gains {
    /// Innovation weight added to the advancing level.
    pub alpha: f64,
    /// Innovation weight added to the trend.
    pub beta: f64,
    /// Innovation weight added to the revisited seasonal phase.
    pub gamma: f64,
}
/// The final ETS state used for forecasts, confidence intervals, and fit statistics.
/// State and error metrics come from the same one-step pass, with the input scale retained for reporting.
#[derive(Debug)]
pub struct Fit {
    level: f64,
    trend: f64,
    seasons: Vec<f64>,
    next_phase: usize,
    gains: Gains,
    sse: f64,
    mae: f64,
    smape: f64,
    naive: f64,
    count: usize,
    scale: f64,
}
/// Normalize values once to keep fitted residual squares finite.
fn normalized<O>(values: &[f64]) -> Result<(Vec<f64>, f64), O> {
    let scale = values
        .iter()
        .map(|x| x.abs())
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE);
    let mut result = buffer(values.len())?;
    for &value in values {
        // SAFETY: values.len slots reserved and each input emits one member.
        result.push(value / scale);
    }
    Ok((result, scale))
}
/// Initialize a level/trend and zero-mean seasonal phases from one or two cycles.
fn initial<O>(values: &[f64], period: usize) -> Result<(f64, f64, Vec<f64>), O> {
    let n = values.len();
    if n < 2 || period > 0 && n / 2 < period {
        return Err(num());
    }
    let mut seasons = buffer(period)?;
    let first = *values.first().ok_or_else(num)?;
    let (level, trend) = if period == 0 {
        (first, values.get(1).copied().ok_or_else(num)? - first)
    } else {
        let a = values.get(..period).ok_or_else(num)?.iter().sum::<f64>() / period as f64;
        let twice = period.checked_mul(2).ok_or_else(allocation)?;
        let b = values
            .get(period..twice)
            .ok_or_else(num)?
            .iter()
            .sum::<f64>()
            / period as f64;
        let trend = (b - a) / period as f64;
        let level = a - trend * (period as f64 - 1.0) * 0.5;
        for (i, value) in values.iter().take(period).enumerate() {
            // SAFETY: period slots reserved and take(period) emits at most period values.
            seasons.push(*value - level - i as f64 * trend);
        }
        (level, trend)
    };
    // State is immediately before the first observation; its first prediction is level.
    Ok((level - trend, trend, seasons))
}
/// Advance one observation using the direct additive innovations equations.
fn innovation(
    level: &mut f64,
    trend: &mut f64,
    season: Option<&mut f64>,
    gains: Gains,
    value: f64,
) -> (f64, f64) {
    let seasonal = season.as_deref().copied().unwrap_or(0.0);
    let prediction = *level + *trend + seasonal;
    let error = value - prediction;
    *level = *level + *trend + gains.alpha * error;
    *trend += gains.beta * error;
    if let Some(season) = season {
        *season += gains.gamma * error;
    }
    (prediction, error)
}
/// Execute the innovations equations, optionally collecting diagnostics.
fn run<O>(
    values: &[f64],
    period: usize,
    gains: Gains,
    diagnostics: bool,
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<Fit, O> {
    meter((values.len() as u64).saturating_mul(if diagnostics { 24 } else { 12 }))
        .map_err(Failure::Operational)?;
    let (mut level, mut trend, mut seasons) = initial(values, period)?;
    let (mut sse, mut absolute, mut smape, mut naive) = (0.0, 0.0, 0.0, 0.0);
    let lag = period.max(1);
    for (i, &value) in values.iter().enumerate() {
        let phase = if period == 0 { 0 } else { i % period };
        let (prediction, error) =
            innovation(&mut level, &mut trend, seasons.get_mut(phase), gains, value);
        sse += error * error;
        if diagnostics {
            absolute += error.abs();
            let denominator = value.abs() + prediction.abs();
            if denominator > 0.0 {
                smape += 2.0 * error.abs() / denominator;
            }
            if let Some(previous) = i.checked_sub(lag).and_then(|j| values.get(j)) {
                naive += (value - previous).abs();
            }
        }
        if !sse.is_finite() || !level.is_finite() || !trend.is_finite() {
            return Err(num());
        }
    }
    let count = values.len();
    Ok(Fit {
        level,
        trend,
        seasons,
        next_phase: if period == 0 { 0 } else { count % period },
        gains,
        sse,
        mae: absolute / count as f64,
        smape: smape / count as f64,
        naive: naive / count.saturating_sub(lag).max(1) as f64,
        count,
        scale: 1.0,
    })
}
/// Convert bounded external weights to stable direct innovations gains.
fn gains(parameters: [f64; 3], seasonal: bool) -> Gains {
    let [alpha, beta, gamma] = parameters;
    Gains {
        alpha,
        beta: alpha * beta,
        gamma: if seasonal { (1.0 - alpha) * gamma } else { 0.0 },
    }
}
/// Estimate gains by deterministic multistart bounded coordinate search.
fn fit_period<O>(
    values: &[f64],
    period: usize,
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<Fit, O> {
    let mut best_parameters = [0.0; 3];
    let mut best_score = f64::INFINITY;
    for start in [
        [0.1, 0.1, 0.1],
        [0.5, 0.5, 0.5],
        [0.9, 0.1, 0.9],
        [0.0, 0.0, 0.0],
    ] {
        let mut parameters = start;
        let mut score = run(values, period, gains(parameters, period > 0), false, meter)?.sse;
        let mut delta = 0.25;
        let mut converged = false;
        let mut small_progress = 0_u8;
        for _ in 0..1024 {
            meter(1).map_err(Failure::Operational)?;
            let before = score;
            for dimension in 0..if period == 0 { 2 } else { 3 } {
                for direction in [-1.0, 1.0] {
                    let mut candidate = parameters;
                    let entry = candidate.get_mut(dimension).ok_or_else(num)?;
                    *entry = (*entry + direction * delta).clamp(0.0, 1.0);
                    if candidate == parameters {
                        continue;
                    }
                    let next = run(values, period, gains(candidate, period > 0), false, meter)?.sse;
                    if next < score {
                        parameters = candidate;
                        score = next;
                    }
                }
            }
            if score == 0.0 {
                converged = true;
                break;
            }
            let progress = (before - score) / before;
            if progress <= 1e-8 && (progress > 0.0 || small_progress > 0) {
                small_progress = small_progress.saturating_add(1);
            } else {
                small_progress = 0;
            }
            if score >= before {
                delta *= 0.5;
            }
            if delta < 1e-5 || small_progress >= 3 {
                converged = true;
                break;
            }
        }
        if converged && score < best_score {
            best_score = score;
            best_parameters = parameters;
        }
    }
    if !best_score.is_finite() {
        return Err(Failure::NonConvergence);
    }
    run(
        values,
        period,
        gains(best_parameters, period > 0),
        true,
        meter,
    )
}
/// Detect the smallest strong repeated pattern after independently detrending values.
fn detect<O>(
    values: &[f64],
    meter: &mut impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<usize, O> {
    let n = values.len();
    if n < 4 {
        return Ok(0);
    }
    meter((n as u64).saturating_mul(16)).map_err(Failure::Operational)?;
    let mean = values.iter().sum::<f64>() / n as f64;
    let center = (n as f64 - 1.0) * 0.5;
    let mut xy = 0.0;
    let mut xx = 0.0;
    for (i, &y) in values.iter().enumerate() {
        let x = i as f64 - center;
        xy += x * (y - mean);
        xx += x * x;
    }
    let slope = xy / xx;
    let mut residuals = buffer(n)?;
    for (i, &y) in values.iter().enumerate() {
        // SAFETY: n slots reserved and values has n members.
        residuals.push(y - mean - slope * (i as f64 - center));
    }
    let base = residuals.iter().map(|x| x * x).sum::<f64>();
    if base <= 64.0 * f64::EPSILON * f64::EPSILON * n as f64 {
        return Ok(0);
    }
    let maximum = (n / 2).min(8760);
    let mut best = 0;
    let mut best_score = base;
    for period in 2..=maximum {
        meter((n as u64).saturating_mul(8)).map_err(Failure::Operational)?;
        let mut error = 0.0;
        for phase in 0..period {
            let mut sum = 0.0;
            let mut count = 0.0;
            for &value in residuals.iter().skip(phase).step_by(period) {
                sum += value;
                count += 1.0;
            }
            let average = sum / count;
            for &value in residuals.iter().skip(phase).step_by(period) {
                error += (value - average).powi(2);
            }
        }
        // Prefer fewer seasonal states; require at least 20% of detrended variation explained.
        let score = error + base * period as f64 / n as f64;
        if score < base * 0.8 && score < best_score {
            best = period;
            best_score = score;
        }
    }
    Ok(best)
}
/// Fit one normalized shared model, charging detection and every optimizer pass.
pub fn fit<O>(
    series: &Series,
    seasonality: Seasonality,
    mut meter: impl FnMut(u64) -> std::result::Result<(), O>,
) -> Result<Fit, O> {
    meter((series.values.len() as u64).saturating_mul(4)).map_err(Failure::Operational)?;
    let (values, scale) = normalized(&series.values)?;
    let period = match seasonality {
        Seasonality::None => 0,
        Seasonality::Period(m) => m,
        Seasonality::Automatic => detect(&values, &mut meter)?,
    };
    if period > 8760 || period == 1 {
        return Err(num());
    }
    let mut fit = fit_period(&values, period, &mut meter)?;
    fit.scale = scale;
    Ok(fit)
}
impl Fit {
    /// Return the actual seasonal state length, or zero for nonseasonal fits.
    pub fn period(&self) -> usize {
        self.seasons.len()
    }
    /// Project an integer nonnegative horizon from the final fitted state.
    fn integer(&self, horizon: f64) -> f64 {
        let seasonal = if self.seasons.is_empty() {
            0.0
        } else {
            let period = self.seasons.len();
            let offset = (horizon - 1.0).rem_euclid(period as f64) as usize;
            // Both addends are below period, whose source limit is 8760.
            self.seasons
                .get(self.next_phase.checked_add(offset).unwrap_or(0) % period)
                .copied()
                .unwrap_or(0.0)
        };
        self.level + horizon * self.trend + seasonal
    }
    /// Forecast a supported horizon, interpolating adjacent seasonal projections.
    pub fn forecast<O>(&self, horizon: f64) -> Result<f64, O> {
        if !horizon.is_finite() || horizon < 0.0 {
            return Err(num());
        }
        if horizon > 1e9 {
            return Err(Failure::ComputationalLimit);
        }
        let lower = horizon.floor();
        let fraction = horizon - lower;
        let value = if fraction == 0.0 {
            self.integer(lower)
        } else {
            (1.0 - fraction) * self.integer(lower) + fraction * self.integer(lower + 1.0)
        };
        let result = value * self.scale;
        if result.is_finite() {
            Ok(result)
        } else {
            Err(num())
        }
    }
    /// Return one declared metric or direct innovations gain for STAT selectors 1..7.
    pub fn statistic<O>(&self, selector: usize) -> Result<f64, O> {
        let result = match selector {
            1 => self.gains.alpha,
            2 => self.gains.beta,
            3 => self.gains.gamma,
            4 => {
                if self.naive > 0.0 {
                    self.mae / self.naive
                } else if self.mae == 0.0 {
                    0.0
                } else {
                    return Err(Failure::Spreadsheet(CellError::Div0));
                }
            }
            5 => self.smape,
            6 => self.mae * self.scale,
            7 => (self.sse / self.count as f64).sqrt() * self.scale,
            _ => return Err(num()),
        };
        if result.is_finite() {
            Ok(result)
        } else {
            Err(num())
        }
    }
    /// Compute a future-observation interval radius from the exact innovations response.
    pub fn confidence<O>(&self, horizon: f64, confidence: f64) -> Result<f64, O> {
        if !confidence.is_finite()
            || confidence <= 0.0
            || confidence >= 1.0
            || !horizon.is_finite()
            || horizon < 0.0
        {
            return Err(num());
        }
        if horizon > 1e9 {
            return Err(Failure::ComputationalLimit);
        }
        if self.sse == 0.0 {
            return Ok(0.0);
        }
        let h = horizon.ceil().max(1.0);
        let Gains {
            alpha: a,
            beta: b,
            gamma: g,
        } = self.gains;
        let k = if self.period() == 0 {
            0.0
        } else {
            ((h - 1.0) / self.period() as f64).floor()
        };
        let m = self.period() as f64;
        let multiplier = 1.0
            + (h - 1.0) * a * a
            + a * b * h * (h - 1.0)
            + b * b * h * (h - 1.0) * (2.0 * h - 1.0) / 6.0
            + g * k * (2.0 * a + g + b * m * (k + 1.0));
        let sigma = (self.sse / self.count as f64 * multiplier).sqrt() * self.scale;
        if confidence < 0.5 {
            // erf_inv takes central coverage directly, avoiding 1-coverage cancellation.
            let radius =
                std::f64::consts::SQRT_2 * statrs::function::erf::erf_inv(confidence) * sigma;
            if radius.is_finite() {
                Ok(radius)
            } else {
                Err(num())
            }
        } else {
            crate::statistics::modern_inverse::confidence(
                1.0 - confidence,
                sigma,
                1.0,
                crate::statistics::modern_inverse::ConfidenceKind::Normal,
                |_| Ok(()),
            )
        }
    }
}

#[cfg(test)]
#[path = "ets_tests.rs"]
mod tests;
