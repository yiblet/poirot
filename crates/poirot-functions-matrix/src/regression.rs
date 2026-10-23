use super::{Budget, Error, Matrix, Result, TrendFitPolicy, finite, reference, transpose};

/// Least squares with Householder QR (never the unstable normal equations).
/// Optional inputs represent missing arguments, not blank cells. The adapter
/// must require numeric TREND elements and a literal boolean fourth argument.
pub(super) fn trend(
    y: &Matrix<f64>,
    known_x: Option<&Matrix<f64>>,
    new_x: Option<&Matrix<f64>>,
    constant: bool,
    budget: Budget,
    policy: TrendFitPolicy,
) -> Result<Matrix<f64>> {
    if policy == TrendFitPolicy::Determined {
        return modern_prediction(y, known_x, new_x, constant, budget);
    }
    let n = y.values.len();
    if n == 1 {
        return Err(Error::SourceUnsupported("Sample size too small"));
    }
    let p = known_x.map_or(1, |x| {
        if y.rows == 1 && x.cols > 1 {
            x.rows
        } else if y.rows > 1 && y.cols > 1 {
            1
        } else {
            x.cols
        }
    });
    let q = p
        .checked_add(usize::from(constant))
        .ok_or(Error::SizeOverflow)?;
    let pred_n = new_x.map_or(n, |x| if p == 1 { x.values.len() } else { x.rows });
    let cells = n
        .checked_mul(q)
        .and_then(|v| v.checked_mul(2))
        .and_then(|v| v.checked_add(q))
        .and_then(|v| v.checked_add(n))
        .and_then(|v| v.checked_add(pred_n.checked_mul(p)?))
        .and_then(|v| v.checked_add(pred_n));
    let work = n
        .checked_mul(q)
        .and_then(|v| v.checked_mul(q))
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(pred_n.checked_mul(p)?));
    budget.check(cells, work)?;
    finite(&y.values)?;
    if let Some(x) = known_x {
        finite(&x.values)?;
    }
    if let Some(x) = new_x {
        finite(&x.values)?;
    }
    let mut x = if let Some(x) = known_x {
        if y.rows == 1 && x.cols > 1 {
            transpose(x, budget)?
        } else if y.rows > 1 && y.cols > 1 {
            Matrix::new(x.values.len(), 1, x.values.clone())?
        } else {
            x.clone()
        }
    } else {
        Matrix::new(n, 1, (1..=n).map(|i| i as f64).collect())?
    };
    if y.rows > 1
        && y.cols > 1
        && (known_x.is_none() || known_x.is_some_and(|k| k.rows != y.rows) || x.rows != n)
    {
        return reference();
    }
    if x.cols >= x.rows {
        return Err(Error::SourceUnsupported("Sample size too small"));
    }
    if x.rows != n {
        return reference();
    }
    let prediction = if let Some(nx) = new_x {
        if x.cols == 1 {
            Matrix::new(nx.values.len(), 1, nx.values.clone())?
        } else {
            nx.clone()
        }
    } else {
        x.clone()
    };
    if prediction.cols != x.cols {
        return reference();
    }
    let (rows, cols) = new_x
        .or(known_x)
        .map_or((y.rows, y.cols), |m| (m.rows, m.cols));
    // The POI wrapper can produce an invalid area size for multiple predictors;
    // preserve a real rectangular prediction vector with y's observation axis.
    let (rows, cols) = if rows.checked_mul(cols) == Some(prediction.rows) {
        (rows, cols)
    } else if y.rows == 1 {
        (1, prediction.rows)
    } else {
        (prediction.rows, 1)
    };
    if x.rows.checked_mul(x.cols) != Some(x.values.len())
        || prediction.rows.checked_mul(prediction.cols) != Some(prediction.values.len())
        || x.cols != p
        || x.rows == 0
    {
        return reference();
    }
    let mut all_same = true;
    for col in 0..x.cols {
        for row in 1..x.rows {
            all_same &= x.get(row, col)? == x.get(0, col)?;
        }
    }
    // The pinned source returns a response mean for constant predictors,
    // including constrained fits. Modern fits returned through the shared kernel above.
    if all_same {
        let mean = y.values.iter().sum::<f64>() / n as f64;
        finite(&[mean])?;
        return Matrix::new(rows, cols, vec![mean; prediction.rows]);
    }
    // The pinned source requires an additional observation beyond its design width.
    if n <= q {
        return reference();
    }
    // Reuse x's storage as the design matrix after preflight.
    if constant {
        let mut design = Vec::with_capacity(n * q);
        for row in 0..n {
            design.push(1.0);
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            design.extend_from_slice(&x.values[row * p..(row + 1) * p]);
        }
        x = Matrix::new(n, q, design)?;
    }
    if x.rows != n || x.cols != q || n.checked_mul(q) != Some(x.values.len()) {
        return reference();
    }
    let mut rhs = y.values.clone();
    let mut diag = vec![0.0; q];
    for (col, diagonal) in diag.iter_mut().enumerate() {
        let norm = (col..n).try_fold(0.0f64, |acc, row| {
            x.get(row, col).map(|value| acc.hypot(*value))
        })?;
        let a = if *x.get(col, col)? > 0.0 { -norm } else { norm };
        if a == 0.0 {
            return Err(Error::SourceUnsupported("Singular matrix in input"));
        }
        *diagonal = a;
        // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
        x.values[col * q + col] -= a;
        for j in col + 1..q {
            let dot = (col..n)
                // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
                .map(|row| x.values[row * q + j] * x.values[row * q + col])
                .sum::<f64>();
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            let alpha = -dot / (a * x.values[col * q + col]);
            for row in col..n {
                // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
                x.values[row * q + j] -= alpha * x.values[row * q + col];
            }
        }
        let dot = (col..n)
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            .map(|row| rhs[row] * x.values[row * q + col])
            .sum::<f64>();
        // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
        let alpha = -dot / (a * x.values[col * q + col]);
        for (row, value) in rhs.iter_mut().enumerate().skip(col) {
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            *value -= alpha * x.values[row * q + col];
        }
    }
    for col in (0..q).rev() {
        // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
        rhs[col] /= diag[col];
        for row in 0..col {
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            rhs[row] -= rhs[col] * x.values[row * q + col];
        }
    }
    let mut out = Vec::with_capacity(prediction.rows);
    for row in 0..prediction.rows {
        // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
        let mut sum = if constant { rhs[0] } else { 0.0 };
        for col in 0..p {
            // SAFETY: Local shape checks require n * p input and n * q design entries; n >= q, and loop row/column bounds stay within these dimensions and rhs/diag lengths.
            sum += rhs[col + usize::from(constant)] * prediction.get(row, col)?;
        }
        out.push(sum);
    }
    finite(&out)?;
    Matrix::new(rows, cols, out)
}

/// Normalize modern worksheet axes and predict through the shared rank-aware fit.
fn modern_prediction(
    y: &Matrix<f64>,
    known_x: Option<&Matrix<f64>>,
    new_x: Option<&Matrix<f64>>,
    constant: bool,
    budget: Budget,
) -> Result<Matrix<f64>> {
    let n = y.values.len();
    let p = known_x.map_or(1, |x| {
        if y.rows == 1 && y.cols > 1 {
            x.rows
        } else if y.cols == 1 {
            x.cols
        } else {
            1
        }
    });
    let predictions = new_x.map_or(n, |x| {
        if p == 1 {
            x.values.len()
        } else if y.rows == 1 {
            x.cols
        } else {
            x.rows
        }
    });
    let (cells, work) = crate::regression_budget(n, p, predictions, false)?;
    budget.check(Some(cells), Some(work))?;
    let clone_matrix = |source: &Matrix<f64>| -> Result<Matrix<f64>> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(source.values.len())
            .map_err(|_| Error::AllocationFailure)?;
        // SAFETY: source length was reserved above; the copy adds exactly that many values.
        values.extend_from_slice(&source.values);
        Matrix::new(source.rows, source.cols, values)
    };
    let x = match known_x {
        Some(x) if y.rows == 1 && y.cols > 1 => {
            if x.cols != n {
                return reference();
            }
            transpose(x, budget)?
        }
        Some(x) if y.cols == 1 => {
            if x.rows != n {
                return reference();
            }
            clone_matrix(x)?
        }
        Some(x) => {
            if x.rows != y.rows || x.cols != y.cols {
                return reference();
            }
            Matrix::new(n, 1, clone_matrix(x)?.into_values())?
        }
        None => {
            let mut values = Vec::new();
            values
                .try_reserve_exact(n)
                .map_err(|_| Error::AllocationFailure)?;
            for i in 0..n {
                // SAFETY: n slots reserved; exactly n values are appended.
                values.push(i as f64 + 1.0);
            }
            Matrix::new(n, 1, values)?
        }
    };
    let mut shape = (y.rows, y.cols);
    let prediction = match new_x {
        Some(x) if p == 1 => {
            shape = (x.rows, x.cols);
            Matrix::new(x.values.len(), 1, clone_matrix(x)?.into_values())?
        }
        Some(x) if y.rows == 1 => {
            if x.rows != p {
                return reference();
            }
            shape = (1, x.cols);
            transpose(x, budget)?
        }
        Some(x) => {
            if x.cols != p {
                return reference();
            }
            shape = (x.rows, 1);
            clone_matrix(x)?
        }
        None => {
            if p > 1 {
                shape = if y.rows == 1 { (1, n) } else { (n, 1) };
            }
            clone_matrix(&x)?
        }
    };
    let fit = crate::least_squares(
        &x,
        &y.values,
        if constant {
            crate::Intercept::Estimate
        } else {
            crate::Intercept::Zero
        },
        budget,
    )?;
    let result = fit.predict_trend(&prediction, budget)?;
    Matrix::new(shape.0, shape.1, result.into_values())
}
