use super::{Budget, Error, Matrix, Result, finite, num, value};

/// Swap rows and columns after admitting output size and work.
pub(super) fn transpose<T: Clone>(m: &Matrix<T>, budget: Budget) -> Result<Matrix<T>> {
    budget.check(Some(m.values.len()), Some(m.values.len()))?;
    let mut values = Vec::with_capacity(m.values.len());
    for col in 0..m.cols {
        for row in 0..m.rows {
            values.push(m.get(row, col)?.clone());
        }
    }
    Matrix::new(m.cols, m.rows, values)
}
/// Multiply conformable numeric matrices under the supplied budget.
pub(super) fn multiply(a: &Matrix<f64>, b: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
    if a.cols != b.rows {
        return value();
    }
    let cells = a.rows.checked_mul(b.cols);
    budget.check(cells, cells.and_then(|n| n.checked_mul(a.cols)))?;
    finite(&a.values)?;
    finite(&b.values)?;
    let mut values = vec![0.0; cells.ok_or(Error::BudgetExceeded)?];
    for i in 0..a.rows {
        for j in 0..b.cols {
            let mut sum = 0.0;
            for k in 0..a.cols {
                sum += a.get(i, k)? * b.get(k, j)?;
            }
            // SAFETY: values is allocated for a.rows * b.cols using a checked product; i and j are bounded by those dimensions.
            values[i * b.cols + j] = sum;
        }
    }
    finite(&values)?;
    Matrix::new(a.rows, b.cols, values)
}

/// Partial-pivot LU, matching Commons Math's default 1e-11 singularity threshold.
fn lu(m: &Matrix<f64>, budget: Budget) -> Result<(Vec<f64>, Vec<usize>, bool, bool)> {
    if m.rows != m.cols {
        return value();
    }
    let n = m.rows;
    if n.checked_mul(n) != Some(m.values.len()) {
        return value();
    }
    budget.check(
        m.values.len().checked_add(n),
        n.checked_mul(n).and_then(|v| v.checked_mul(n)),
    )?;
    finite(&m.values)?;
    let mut a = m.values.clone();
    let mut pivots: Vec<usize> = (0..n).collect();
    let mut odd = false;
    for col in 0..n {
        // Crout's column update has the same summation order as Commons Math LU.
        for row in 0..n {
            // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
            let mut sum = a[row * n + col];
            for k in 0..row.min(col) {
                // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
                sum -= a[row * n + k] * a[k * n + col];
            }
            // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
            a[row * n + col] = sum;
        }
        let mut pivot = col;
        for row in col + 1..n {
            // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
            if a[row * n + col].abs() > a[pivot * n + col].abs() {
                pivot = row;
            }
        }
        // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
        if a[pivot * n + col].abs() < 1e-11 {
            return Ok((a, pivots, odd, true));
        }
        if pivot != col {
            for k in 0..n {
                // SAFETY: a.len() was checked to equal n * n; pivot and col are below n, and k ranges over 0..n.
                a.swap(pivot * n + k, col * n + k);
            }
            // SAFETY: pivots was collected from 0..n; pivot is selected from col..n and col is below n.
            pivots.swap(pivot, col);
            odd = !odd;
        }
        for row in col + 1..n {
            // SAFETY: This function checks a has n * n entries; row/col/k and the pivot selected from col..n are all less than n.
            a[row * n + col] /= a[col * n + col];
        }
    }
    finite(&a)?;
    Ok((a, pivots, odd, false))
}
/// Compute a square matrix determinant under the supplied budget.
pub(super) fn determinant(m: &Matrix<f64>, budget: Budget) -> Result<f64> {
    let (a, _, odd, singular) = lu(m, budget)?;
    if singular {
        return Ok(0.0);
    }
    if m.rows != m.cols || m.rows.checked_mul(m.cols) != Some(a.len()) {
        return value();
    }
    let mut d = if odd { -1.0 } else { 1.0 };
    for i in 0..m.rows {
        // SAFETY: The local guard requires a square matrix and a.len() == rows * cols; i ranges strictly below rows.
        d *= a[i * m.cols + i];
    }
    finite(&[d])?;
    Ok(d)
}
/// Compute a square matrix inverse, rejecting singular or invalid inputs.
pub(super) fn inverse(m: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
    let n = m.rows;
    budget.check(
        m.values.len().checked_mul(2).and_then(|v| v.checked_add(n)),
        n.checked_mul(n)
            .and_then(|v| v.checked_mul(n))
            .and_then(|v| v.checked_mul(3)),
    )?;
    let (a, pivots, _, singular) = lu(m, budget)?;
    if singular {
        return num();
    }
    let cells = n.checked_mul(n).ok_or(Error::BudgetExceeded)?;
    if a.len() != cells || pivots.len() != n || pivots.iter().any(|&pivot| pivot >= n) {
        return value();
    }
    let mut out = vec![0.0; cells];
    for i in 0..n {
        // SAFETY: The local checks validate a/out have n * n entries and every pivot is below n; all loop coordinates are in 0..n.
        out[i * n + pivots[i]] = 1.0;
    }
    for col in 0..n {
        for row in col + 1..n {
            for j in 0..n {
                // SAFETY: The local checks validate a/out have n * n entries and every pivot is below n; all loop coordinates are in 0..n.
                out[row * n + j] -= out[col * n + j] * a[row * n + col];
            }
        }
    }
    for col in (0..n).rev() {
        for j in 0..n {
            // SAFETY: The local checks validate a/out have n * n entries and every pivot is below n; all loop coordinates are in 0..n.
            out[col * n + j] /= a[col * n + col];
        }
        for row in 0..col {
            for j in 0..n {
                // SAFETY: The local checks validate a/out have n * n entries and every pivot is below n; all loop coordinates are in 0..n.
                out[row * n + j] -= out[col * n + j] * a[row * n + col];
            }
        }
    }
    finite(&out)?;
    Matrix::new(n, n, out)
}

/// POI uses Java binarySearch directly, including its behavior for duplicate or
/// unsorted bins. Do not sort or deduplicate bins in the adapter.
pub(super) fn frequency(data: &[f64], bins: &[f64], budget: Budget) -> Result<Matrix<f64>> {
    let cells = bins.len().checked_add(1);
    let search_work = (usize::BITS - bins.len().leading_zeros()) as usize + 1;
    budget.check(
        cells,
        data.len()
            .checked_mul(search_work)
            .and_then(|n| n.checked_add(bins.len())),
    )?;
    finite(data)?;
    finite(bins)?;
    let mut out = vec![0.0; cells.ok_or(Error::BudgetExceeded)?];
    for &x in data {
        let mut lo = 0;
        let mut hi = bins.len();
        let mut found = None;
        while lo < hi {
            let mid = lo + (hi - lo - 1) / 2;
            // SAFETY: The binary-search bounds stay within bins; out has bins.len() + 1 entries, including the insertion point at bins.len().
            if bins[mid] < x {
                lo = mid + 1;
            // SAFETY: The binary-search bounds stay within bins; out has bins.len() + 1 entries, including the insertion point at bins.len().
            } else if bins[mid] > x {
                hi = mid;
            } else {
                found = Some(mid);
                break;
            }
        }
        // SAFETY: The binary-search bounds stay within bins; out has bins.len() + 1 entries, including the insertion point at bins.len().
        out[found.unwrap_or(lo)] += 1.0;
    }
    Matrix::new(out.len(), 1, out)
}
