//! Modern regression shape admission and typed worksheet presentation.
use super::*;
use crate::host::MissingPolicy;
use crate::registry::MatrixFunction as F;
use poirot_functions_matrix::{self as kernel, Intercept, Matrix, Statistic};

fn collect(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<Option<Matrix<f64>>> {
    let shape = host.describe(index)?;
    if shape.missing {
        return Ok(None);
    }
    if shape.sheets != 1 {
        return Err(VALUE_ERROR.into());
    }
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    host.charge_work(count as u64)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for r in 0..shape.rows {
        for c in 0..shape.columns {
            let cell = host.read_cell(index, r, c)?;
            let value = match cell.value {
                Number(v) => v,
                Error(e) => return Err(e.into()),
                _ => return Err(VALUE_ERROR.into()),
            };
            // SAFETY: count is the checked rectangle size, reserved above; loops append count values.
            values.push(value);
        }
    }
    Matrix::new(shape.rows, shape.columns, values)
        .map(Some)
        .map_err(|e| modern::failure(host, e))
}
fn option(
    host: &mut CallHost<'_, '_>,
    index: usize,
    length: usize,
    default: bool,
) -> FunctionResult<bool> {
    if index >= length {
        return Ok(default);
    }
    let value = host.scalar_data(index, MissingPolicy::Blank)?;
    if value.missing {
        return Ok(default);
    }
    match value.value {
        Boolean(b) => Ok(b),
        v => host
            .coercev2_scalar_number(&v, NumericPolicy::PinnedPoi)
            .map(|n| n != 0.0),
    }
}
/// Normalize worksheet observation axes without filtering or rearranging member associations.
fn predictors(y: &Matrix<f64>, x: Matrix<f64>) -> kernel::Result<Matrix<f64>> {
    let n = y.values().len();
    if y.rows() == 1 && y.cols() > 1 {
        if x.cols() != n {
            return Err(kernel::Error::Formula(CellError::Ref));
        }
        kernel::transpose(&x, UNLIMITED)
    } else if y.cols() == 1 {
        if x.rows() != n {
            return Err(kernel::Error::Formula(CellError::Ref));
        }
        Ok(x)
    } else {
        if x.rows() != y.rows() || x.cols() != y.cols() {
            return Err(kernel::Error::Formula(CellError::Ref));
        }
        Matrix::new(n, 1, x.into_values())
    }
}
const UNLIMITED: kernel::Budget = kernel::Budget {
    max_cells: usize::MAX,
    max_work: usize::MAX,
};
fn admit(
    host: &mut CallHost<'_, '_>,
    n: usize,
    p: usize,
    m: usize,
    stats: bool,
) -> FunctionResult<()> {
    let (cells, work) =
        kernel::regression_budget(n, p, m, stats).map_err(|e| modern::failure(host, e))?;
    host.check_values(cells)?;
    Ok(host.charge_work(work as u64)?)
}
fn value(stat: Statistic) -> CellValue {
    match stat {
        Ok(v) => Number(v),
        Err(e) => Error(e),
    }
}
fn exponential(stat: Statistic) -> Statistic {
    stat.and_then(|v| {
        let v = v.exp();
        if v.is_finite() { Ok(v) } else { Err(Num) }
    })
}
fn evaluate(
    host: &mut CallHost<'_, '_>,
    function: F,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    let y = collect(host, 0)?.ok_or(FunctionFailure::Spreadsheet(VALUE_ERROR))?;
    let x = if length > 1 { collect(host, 1)? } else { None };
    let prediction = matches!(function, F::Growth | F::Trend);
    let nx = if prediction && length > 2 {
        collect(host, 2)?
    } else {
        None
    };
    let constant = option(host, if prediction { 3 } else { 2 }, length, true)?;
    let stats = if prediction {
        false
    } else {
        option(host, 3, length, false)?
    };
    let n = y.values().len();
    let predictors_count = x.as_ref().map_or(1, |x| {
        if y.rows() == 1 && y.cols() > 1 {
            x.rows()
        } else if y.cols() == 1 {
            x.cols()
        } else {
            1
        }
    });
    let prediction_bound = if prediction {
        nx.as_ref().map_or(n, |x| {
            if predictors_count == 1 {
                x.values().len()
            } else if y.rows() == 1 {
                x.cols()
            } else {
                x.rows()
            }
        })
    } else {
        0
    };
    admit(host, n, predictors_count, prediction_bound, stats)?;
    let x = match x {
        Some(x) => predictors(&y, x).map_err(|e| modern::failure(host, e))?,
        None => {
            host.check_values(n)?;
            let mut v = Vec::new();
            v.try_reserve_exact(n)
                .map_err(|_| host.allocation_failure())?;
            for i in 0..n {
                // SAFETY: n slots reserved; exactly n default observations appended.
                v.push(i as f64 + 1.0);
            }
            Matrix::new(n, 1, v).map_err(|e| modern::failure(host, e))?
        }
    };
    let p = x.cols();
    let mut output_shape = (y.rows(), y.cols());
    let nx = if prediction {
        match nx {
            Some(nx) => {
                if p == 1 {
                    output_shape = (nx.rows(), nx.cols());
                    Some(
                        Matrix::new(nx.values().len(), 1, nx.into_values())
                            .map_err(|e| modern::failure(host, e))?,
                    )
                } else if y.rows() == 1 {
                    if nx.rows() != p {
                        return Err(CellError::Ref.into());
                    }
                    output_shape = (1, nx.cols());
                    Some(kernel::transpose(&nx, UNLIMITED).map_err(|e| modern::failure(host, e))?)
                } else {
                    if nx.cols() != p {
                        return Err(CellError::Ref.into());
                    }
                    output_shape = (nx.rows(), 1);
                    Some(nx)
                }
            }
            None => {
                if p > 1 {
                    output_shape = if y.rows() == 1 { (1, n) } else { (n, 1) };
                }
                None
            }
        }
    } else {
        None
    };

    let mut response = y.into_values();
    let logarithmic = matches!(function, F::Growth | F::Logest);
    if logarithmic {
        for v in &mut response {
            if *v <= 0.0 {
                return Err(Num.into());
            }
            *v = v.ln();
        }
    }
    let fit = kernel::least_squares(
        &x,
        &response,
        if constant {
            Intercept::Estimate
        } else {
            Intercept::Zero
        },
        UNLIMITED,
    )
    .map_err(|e| modern::failure(host, e))?;
    if prediction {
        let predicted = if function == F::Trend {
            fit.predict_trend(nx.as_ref().unwrap_or(&x), UNLIMITED)
        } else {
            fit.predict(nx.as_ref().unwrap_or(&x), UNLIMITED)
        }
        .map_err(|e| modern::failure(host, e))?;
        let values = predicted.into_values();
        return array(
            host,
            output_shape.0,
            output_shape.1,
            values.into_iter().map(|v| {
                value(if logarithmic {
                    exponential(Ok(v))
                } else {
                    Ok(v)
                })
            }),
            address,
        );
    }
    let width = p.checked_add(1).ok_or_else(|| host.size_overflow())?;
    let rows = if stats { 5 } else { 1 };
    let count = width
        .checked_mul(rows)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for coefficient in fit
        .coefficients()
        .map_err(|e| modern::failure(host, e))?
        .into_iter()
        .rev()
        .chain(std::iter::once(fit.intercept()))
    {
        // SAFETY: count >= p+1; this loop emits exactly p+1 coefficients.
        values.push(value(if logarithmic {
            exponential(coefficient)
        } else {
            coefficient
        }));
    }
    if stats {
        let d = fit
            .diagnostics(UNLIMITED)
            .map_err(|e| modern::failure(host, e))?;
        for se in d
            .standard_errors
            .into_iter()
            .rev()
            .chain(std::iter::once(d.intercept_error))
        {
            // SAFETY: five rows reserved; second row emits width entries.
            values.push(value(se));
        }
        for (left, right) in [
            (d.r_squared, d.residual_error),
            (d.f, Ok(d.degrees as f64)),
            (d.regression_squares, d.residual_squares),
        ] {
            for c in 0..width {
                // SAFETY: three remaining width-sized rows fit the count slots reserved above.
                values.push(value(match c {
                    0 => left,
                    1 => right,
                    _ => Err(CellError::NA),
                }));
            }
        }
    }
    array(host, rows, width, values.into_iter(), address)
}
fn array(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    cols: usize,
    mut values: impl Iterator<Item = CellValue>,
    address: CellAddress,
) -> FunctionResult<Value> {
    host.check_values(rows)?;
    let mut out = Vec::new();
    out.try_reserve_exact(rows)
        .map_err(|_| host.allocation_failure())?;
    for _ in 0..rows {
        let mut row = Vec::new();
        row.try_reserve_exact(cols)
            .map_err(|_| host.allocation_failure())?;
        for _ in 0..cols {
            let v = values.next().ok_or(CellError::Ref)?;
            // SAFETY: cols slots reserved, exactly cols values appended.
            row.push(v);
        }
        // SAFETY: rows slots reserved, exactly rows records appended.
        out.push(row);
    }
    if values.next().is_some() {
        return Err(CellError::Ref.into());
    }
    Ok(Value::Array(ArrayView::anchored(out, Some(address))))
}
/// Run modern fit and prediction functions with admitted scratch and uncatchable host failures.
pub(super) fn execute(
    function: F,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let length = args.len();
    if !(1..=4).contains(&length) {
        return Ok(err(VALUE_ERROR));
    }
    let arrays = if matches!(function, F::Growth | F::Trend) {
        3
    } else {
        2
    };
    let mut host = CallHost::new_array(session, sheet, address, args, arrays);
    match evaluate(&mut host, function, length, address) {
        Ok(v) => Ok(v),
        Err(FunctionFailure::Spreadsheet(e)) => Ok(err(e)),
        Err(e) => Err(operational_only(e)),
    }
}
