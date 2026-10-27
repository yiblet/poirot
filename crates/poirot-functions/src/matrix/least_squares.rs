//! Independently written, scaled column-pivoted Householder least squares.
//! Mathematical definitions and declared numerical policy are in REGRESSION_SOURCES.md.
use super::{Budget, Matrix, MatrixError, Result, finite};
use crate::scalar::CellError;

/// Whether a fit includes an estimated intercept or is constrained through zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Intercept {
    /// Estimate an intercept from centered observations.
    Estimate,
    /// Constrain the model to pass through zero.
    Zero,
}
/// A statistic that can be unavailable independently of the fitted coefficients.
pub type Statistic = std::result::Result<f64, CellError>;
/// Statistics computed from a least-squares fit in original predictor order and response units.
/// Each statistic can carry its own spreadsheet error without discarding the fitted coefficients.
#[derive(Debug)]
pub struct Diagnostics {
    /// Predictor standard errors in original input order; removed predictors are zero.
    pub standard_errors: Vec<Statistic>,
    /// Intercept standard error, or NA for a constrained intercept.
    pub intercept_error: Statistic,
    /// Explained fraction of centered or through-zero total energy.
    pub r_squared: Statistic,
    /// Residual standard deviation in response units.
    pub residual_error: Statistic,
    /// Overall regression F statistic.
    pub f: Statistic,
    /// Residual degrees of freedom after predictor rank removal.
    pub degrees: usize,
    /// Explained sum of squares in response units squared.
    pub regression_squares: Statistic,
    /// Direct residual sum of squares in response units squared.
    pub residual_squares: Statistic,
}
/// A least-squares model used for coefficient reporting, prediction, and optional diagnostics.
/// Centered coordinates and the retained triangular factor allow these results to be computed without refitting.
#[derive(Debug)]
pub struct Fit {
    coefficients: Vec<f64>,
    centers: Vec<Center>,
    response: Center,
    intercept: Intercept,
    triangular: Matrix<f64>,
    permutation: Vec<usize>,
    rank: usize,
    observations: usize,
    residual: f64,
    total: f64,
}
/// Normalization for one least-squares predictor or response.
/// Subtracting an anchor before scaling preserves nearby differences that a large common offset could otherwise erase.
#[derive(Clone, Copy, Debug)]
struct Center {
    anchor: f64,
    scale: f64,
    mean: f64,
}
impl Center {
    fn normalize(&self, value: f64) -> f64 {
        let difference = value - self.anchor;
        let scaled = if difference.is_finite() {
            difference / self.scale
        } else {
            value / self.scale - self.anchor / self.scale
        };
        scaled - self.mean
    }
    fn mean(&self) -> f64 {
        self.scale.mul_add(self.mean, self.anchor)
    }
}
fn alloc<T: Copy>(n: usize, value: T) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(n)
        .map_err(|_| MatrixError::AllocationFailure)?;
    for _ in 0..n {
        // SAFETY: n slots were reserved above; the loop appends exactly n copied values.
        out.push(value);
    }
    Ok(out)
}
fn at<T: Copy>(v: &[T], i: usize) -> Result<T> {
    v.get(i)
        .copied()
        .ok_or(MatrixError::Formula(CellError::Ref))
}
fn put<T>(v: &mut [T], i: usize, value: T) -> Result<()> {
    *v.get_mut(i).ok_or(MatrixError::Formula(CellError::Ref))? = value;
    Ok(())
}
fn set(m: &mut Matrix<f64>, r: usize, c: usize, value: f64) -> Result<()> {
    let i = r
        .checked_mul(m.cols)
        .and_then(|i| i.checked_add(c))
        .ok_or(MatrixError::SizeOverflow)?;
    put(&mut m.values, i, value)
}
fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut s, mut correction) = (0.0, 0.0);
    for v in values {
        let next = s + v;
        correction += if s.abs() >= v.abs() {
            (s - next) + v
        } else {
            (v - next) + s
        };
        s = next;
    }
    s + correction
}
fn center(values: &[f64], intercept: Intercept) -> Result<Center> {
    let anchor = if intercept == Intercept::Estimate {
        at(values, 0)?
    } else {
        0.0
    };
    let mut scale = values
        .iter()
        .map(|v| (v - anchor).abs())
        .fold(0.0, f64::max);
    if !scale.is_finite() {
        scale = values.iter().map(|v| v.abs()).fold(0.0, f64::max);
    }
    if scale == 0.0 {
        scale = 1.0;
    }
    let mut out = Center {
        anchor,
        scale,
        mean: 0.0,
    };
    if intercept == Intercept::Estimate {
        out.mean = sum(values
            .iter()
            .map(|v| out.normalize(*v) / values.len() as f64));
    }
    Ok(out)
}
/// Estimate storage/work before allocating a fit, predictions or optional covariance scratch.
pub fn regression_budget(
    n: usize,
    p: usize,
    predictions: usize,
    diagnostics: bool,
) -> Result<(usize, usize)> {
    let np = n.checked_mul(p).ok_or(MatrixError::SizeOverflow)?;
    let pp = p.checked_mul(p).ok_or(MatrixError::SizeOverflow)?;
    let mp = predictions
        .checked_mul(p)
        .ok_or(MatrixError::SizeOverflow)?;
    let cells = np
        .checked_mul(4)
        .and_then(|v| v.checked_add(pp))
        .and_then(|v| v.checked_add(n.checked_mul(3)?))
        .and_then(|v| v.checked_add(p.checked_mul(16)?))
        .and_then(|v| v.checked_add(mp))
        .and_then(|v| v.checked_add(predictions))
        .ok_or(MatrixError::SizeOverflow)?;
    let work = np
        .checked_mul(n.min(p))
        .and_then(|v| v.checked_mul(20))
        .and_then(|v| v.checked_add(np.checked_mul(8)?))
        .and_then(|v| v.checked_add(mp.checked_mul(16)?))
        .and_then(|v| {
            v.checked_add(if diagnostics {
                pp.checked_mul(p)?.checked_mul(4)?
            } else {
                0
            })
        })
        .ok_or(MatrixError::SizeOverflow)?;
    Ok((cells, work))
}
/// Fit observations-by-predictors with deterministic relative-rank column-pivoted QR.
/// Columns are centered only when estimating an intercept. No normal equations are formed.
pub fn least_squares(
    x: &Matrix<f64>,
    y: &[f64],
    intercept: Intercept,
    budget: Budget,
) -> Result<Fit> {
    let (n, p) = (x.rows, x.cols);
    if y.len() != n {
        return Err(MatrixError::Formula(CellError::Ref));
    }
    let (cells, work) = regression_budget(n, p, 0, false)?;
    budget.check(Some(cells), Some(work))?;
    finite(&x.values)?;
    finite(y)?;
    let response = center(y, intercept)?;
    let mut rhs = alloc(n, 0.0)?;
    for (slot, value) in rhs.iter_mut().zip(y) {
        *slot = response.normalize(*value);
    }
    let total = sum(rhs.iter().map(|v| v * v));
    let mut centers = Vec::new();
    centers
        .try_reserve_exact(p)
        .map_err(|_| MatrixError::AllocationFailure)?;
    let mut a = Matrix::new(n, p, alloc(x.values.len(), 0.0)?)?;
    let mut column = alloc(n, 0.0)?;
    for c in 0..p {
        for (r, slot) in column.iter_mut().enumerate() {
            *slot = *x.get(r, c)?;
        }
        let centered = center(&column, intercept)?;
        for (r, value) in column.iter().enumerate() {
            set(&mut a, r, c, centered.normalize(*value))?;
        }
        // SAFETY: p slots reserved; one center is appended per column.
        centers.push(centered);
    }
    let mut permutation = alloc(p, 0usize)?;
    for (i, slot) in permutation.iter_mut().enumerate() {
        *slot = i;
    }
    let mut rank = 0;
    let mut initial_norm = 0.0;
    for k in 0..n.min(p) {
        let next = k.checked_add(1).ok_or(MatrixError::SizeOverflow)?;
        let mut pivot = k;
        let mut norm = 0.0;
        for c in k..p {
            let candidate = (k..n).try_fold(0.0f64, |acc, r| {
                Ok::<_, MatrixError>(acc.hypot(*a.get(r, c)?))
            })?;
            if candidate > norm {
                norm = candidate;
                pivot = c;
            }
        }
        if k == 0 {
            initial_norm = norm;
        }
        if norm <= initial_norm * (n.max(p) as f64 * 16.0 * f64::EPSILON) {
            break;
        }
        for r in 0..n {
            let old = *a.get(r, k)?;
            let replacement = *a.get(r, pivot)?;
            set(&mut a, r, k, replacement)?;
            set(&mut a, r, pivot, old)?;
        }
        let old = at(&permutation, k)?;
        let replacement = at(&permutation, pivot)?;
        put(&mut permutation, k, replacement)?;
        put(&mut permutation, pivot, old)?;
        let diagonal = *a.get(k, k)?;
        let alpha = -norm.copysign(diagonal);
        let first = diagonal - alpha;
        // Normalized Householder vector starts at one, avoiding a squared large norm.
        for r in next..n {
            let v = *a.get(r, k)? / first;
            set(&mut a, r, k, v)?;
        }
        let tau = (alpha - diagonal) / alpha;
        set(&mut a, k, k, alpha)?;
        for c in next..p {
            let mut dot = *a.get(k, c)?;
            for r in next..n {
                dot += a.get(r, k)? * a.get(r, c)?;
            }
            let change = tau * dot;
            let value = *a.get(k, c)? - change;
            set(&mut a, k, c, value)?;
            for r in next..n {
                let value = *a.get(r, c)? - a.get(r, k)? * change;
                set(&mut a, r, c, value)?;
            }
        }
        let mut dot = at(&rhs, k)?;
        for r in next..n {
            dot += a.get(r, k)? * at(&rhs, r)?;
        }
        let change = tau * dot;
        let value = at(&rhs, k)? - change;
        put(&mut rhs, k, value)?;
        for r in next..n {
            let value = at(&rhs, r)? - a.get(r, k)? * change;
            put(&mut rhs, r, value)?;
        }
        rank = next;
    }
    let mut beta = alloc(p, 0.0)?;
    for k in (0..rank).rev() {
        let mut value = at(&rhs, k)?;
        for c in k.saturating_add(1)..rank {
            value -= a.get(k, c)? * at(&beta, c)?;
        }
        put(&mut beta, k, value / a.get(k, k)?)?;
    }
    let mut coefficients = alloc(p, 0.0)?;
    for k in 0..rank {
        put(&mut coefficients, at(&permutation, k)?, at(&beta, k)?)?;
    }
    // Calculate residuals in centered coordinates; never subtract total and explained squares.
    let mut residuals = alloc(n, 0.0)?;
    for (r, slot) in residuals.iter_mut().enumerate() {
        let mut predicted = 0.0;
        for c in 0..p {
            predicted =
                at(&coefficients, c)?.mul_add(at(&centers, c)?.normalize(*x.get(r, c)?), predicted);
        }
        *slot = response.normalize(at(y, r)?) - predicted;
    }
    let residual = sum(residuals.iter().map(|v| v * v));
    finite(&coefficients)?;
    finite(&[total, residual])?;
    Ok(Fit {
        coefficients,
        centers,
        response,
        intercept,
        triangular: a,
        permutation,
        rank,
        observations: n,
        residual,
        total,
    })
}
impl Fit {
    /// Return the retained predictor rank, excluding the optional intercept.
    pub fn rank(&self) -> usize {
        self.rank
    }
    /// Identify retained predictors independently of whether their fitted slope is zero.
    pub fn active_predictors(&self) -> Result<Vec<bool>> {
        let mut active = alloc(self.coefficients.len(), false)?;
        for &original in self.permutation.iter().take(self.rank) {
            put(&mut active, original, true)?;
        }
        Ok(active)
    }
    /// Project slopes into response units, preserving zero for removed predictors.
    pub fn coefficients(&self) -> Result<Vec<Statistic>> {
        let mut out = alloc(self.coefficients.len(), Ok(0.0))?;
        for (slot, (b, c)) in out
            .iter_mut()
            .zip(self.coefficients.iter().zip(&self.centers))
        {
            *slot = statistic(project(*b, self.response.scale, c.scale));
        }
        Ok(out)
    }
    /// Recover the fitted intercept; through-zero fits return zero.
    pub fn intercept(&self) -> Statistic {
        if self.intercept == Intercept::Zero {
            return Ok(0.0);
        }
        let mut value = self.response.mean();
        for (b, c) in self.coefficients.iter().zip(&self.centers) {
            value -= statistic(project(*b, self.response.scale, c.scale))? * c.mean();
        }
        statistic(value)
    }
    /// Apply TREND's observation/rank admission before using the common predictor.
    /// The retained independent Calc controls require two observations for an
    /// estimated intercept and a nonzero predictor rank for a through-zero fit.
    pub fn predict_trend(&self, x: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
        if (self.intercept == Intercept::Estimate && self.observations < 2)
            || (self.intercept == Intercept::Zero && self.rank == 0)
        {
            return Err(MatrixError::Formula(CellError::Value));
        }
        self.predict(x, budget)
    }
    /// Predict in centered coordinates, independently of coefficient presentation overflow.
    pub fn predict(&self, x: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
        if x.cols != self.coefficients.len() {
            return Err(MatrixError::Formula(CellError::Ref));
        }
        let work = x.values.len().checked_mul(16);
        budget.check(Some(x.rows), work)?;
        finite(&x.values)?;
        let mut out = alloc(x.rows, 0.0)?;
        for (r, slot) in out.iter_mut().enumerate() {
            let mut value = 0.0;
            for c in 0..x.cols {
                let coefficient = at(&self.coefficients, c)?;
                // An algebraic zero must not normalize an irrelevant coordinate
                // that can overflow outside the observed predictor scale.
                if coefficient == 0.0 {
                    continue;
                }
                value = coefficient.mul_add(at(&self.centers, c)?.normalize(*x.get(r, c)?), value);
            }
            let result = self
                .response
                .scale
                .mul_add(value + self.response.mean, self.response.anchor);
            *slot = if result.is_finite() {
                result
            } else {
                self.predict_unscaled(x, r)?
            };
        }
        finite(&out)?;
        Matrix::new(x.rows, 1, out)
    }
    /// Recover a representable prediction when normalized extrapolation overflows.
    fn predict_unscaled(&self, x: &Matrix<f64>, row: usize) -> Result<f64> {
        let mut result = self.response.mean();
        let mut correction = 0.0;
        for c in 0..x.cols {
            let coefficient = at(&self.coefficients, c)?;
            if coefficient == 0.0 {
                continue;
            }
            let center = at(&self.centers, c)?;
            let slope = project(coefficient, self.response.scale, center.scale);
            let input = *x.get(row, c)?;
            let mean = center.mean();
            let delta = input - mean;
            let term = if delta.is_finite() {
                slope * delta
            } else {
                slope * input - slope * mean
            };
            let next = result + term;
            correction += if result.abs() >= term.abs() {
                (result - next) + term
            } else {
                (term - next) + result
            };
            result = next;
        }
        let result = result + correction;
        finite(&[result])?;
        Ok(result)
    }
    /// Compute optional covariance and residual diagnostics by triangular solves.
    pub fn diagnostics(&self, budget: Budget) -> Result<Diagnostics> {
        let p = self.coefficients.len();
        let (cells, work) = regression_budget(self.observations, p, 0, true)?;
        budget.check(Some(cells), Some(work))?;
        let used = self
            .rank
            .checked_add(usize::from(self.intercept == Intercept::Estimate))
            .ok_or(MatrixError::SizeOverflow)?;
        let degrees = self.observations.saturating_sub(used);
        let variance = if degrees == 0 {
            Err(CellError::Div0)
        } else {
            Ok(self.residual / degrees as f64)
        };
        let mut errors = alloc(p, Ok(0.0))?;
        let mut inverse = Matrix::new(
            p,
            p,
            alloc(p.checked_mul(p).ok_or(MatrixError::SizeOverflow)?, 0.0)?,
        )?;
        for j in 0..self.rank {
            for k in (0..self.rank).rev() {
                let mut v = f64::from(k == j);
                for c in k.saturating_add(1)..self.rank {
                    v -= self.triangular.get(k, c)? * inverse.get(c, j)?;
                }
                set(&mut inverse, k, j, v / self.triangular.get(k, k)?)?;
            }
        }
        let mut intercept_variance = 1.0 / self.observations as f64;
        for k in 0..self.rank {
            let original = at(&self.permutation, k)?;
            let norm = (0..self.rank).try_fold(0.0f64, |v, j| {
                Ok::<_, MatrixError>(v.hypot(*inverse.get(k, j)?))
            })?;
            let scale = at(&self.centers, original)?.scale;
            let se = variance
                .and_then(|v| statistic(project(v.sqrt() * norm, self.response.scale, scale)));
            put(&mut errors, original, se)?;
        }
        if self.intercept == Intercept::Estimate {
            for j in 0..self.rank {
                let mut projection = 0.0;
                for k in 0..self.rank {
                    let center = at(&self.centers, at(&self.permutation, k)?)?;
                    projection +=
                        (center.anchor / center.scale + center.mean) * inverse.get(k, j)?;
                }
                intercept_variance += projection * projection;
            }
        }
        let explained = (self.total - self.residual).max(0.0);
        Ok(Diagnostics {
            standard_errors: errors,
            intercept_error: if self.intercept == Intercept::Zero {
                Err(CellError::NA)
            } else {
                variance.and_then(|v| {
                    statistic(v.sqrt() * intercept_variance.sqrt() * self.response.scale)
                })
            },
            r_squared: if self.total == 0.0 {
                Err(CellError::Div0)
            } else {
                statistic(explained / self.total)
            },
            residual_error: variance.and_then(|v| statistic(v.sqrt() * self.response.scale)),
            f: if self.rank == 0 || self.residual == 0.0 || degrees == 0 {
                Err(CellError::Div0)
            } else {
                statistic((explained / self.rank as f64) / (self.residual / degrees as f64))
            },
            degrees,
            regression_squares: statistic((explained * self.response.scale) * self.response.scale),
            residual_squares: statistic(
                (self.residual * self.response.scale) * self.response.scale,
            ),
        })
    }
}
/// Project a normalized value without overflowing a cancellable intermediate ratio.
fn project(value: f64, numerator: f64, denominator: f64) -> f64 {
    let ratio = numerator / denominator;
    if ratio.is_finite() && ratio != 0.0 {
        value * ratio
    } else {
        (value * numerator) / denominator
    }
}
fn statistic(value: f64) -> Statistic {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(CellError::Num)
    }
}

#[cfg(test)]
#[path = "least_squares_tests.rs"]
mod tests;
