use super::{Budget, Matrix, MatrixError, Result, reference, transpose};

/// Least squares with Householder QR (never the unstable normal equations).
/// Optional inputs represent missing arguments, not blank cells. The adapter
/// must require numeric TREND elements and a literal boolean fourth argument.
pub(super) fn trend(
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
    let (cells, work) = crate::matrix::regression_budget(n, p, predictions, false)?;
    budget.check(Some(cells), Some(work))?;
    let clone_matrix = |source: &Matrix<f64>| -> Result<Matrix<f64>> {
        let mut values = Vec::new();
        values
            .try_reserve_exact(source.values.len())
            .map_err(|_| MatrixError::AllocationFailure)?;
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
                .map_err(|_| MatrixError::AllocationFailure)?;
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
    let fit = crate::matrix::least_squares(
        &x,
        &y.values,
        if constant {
            crate::matrix::Intercept::Estimate
        } else {
            crate::matrix::Intercept::Zero
        },
        budget,
    )?;
    let result = fit.predict_trend(&prediction, budget)?;
    Matrix::new(shape.0, shape.1, result.into_values())
}
