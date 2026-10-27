//! Modern regression shape admission and typed worksheet presentation.
use crate::eval::adapters::elementwise::matrices;
use crate::eval::adapters::elementwise::matrices::array;
use crate::eval::adapters::failure::operational_only;
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::{CallHost, OperandReader};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Num;
use crate::model::CellValue::{Error, Number};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::matrix::{self as kernel, Intercept, Matrix, Statistic};

fn collect(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<Option<Matrix<f64>>> {
    let shape = OperandReader::new(host).describe(index)?;
    if shape.missing {
        return Ok(None);
    }
    crate::eval::adapters::elementwise::matrices::numeric(host, index).map(Some)
}

fn option(host: &mut CallHost<'_, '_>, index: usize, default: bool) -> FunctionResult<bool> {
    Ok(crate::eval::adapters::elementwise::NumericSignature::new(&[
        crate::eval::adapters::elementwise::NumericParameter::boolean_default(default),
    ])
    .scalars(host, index)?
    .get(0)?
        != 0.0)
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
        kernel::regression_budget(n, p, m, stats).map_err(|e| matrices::failure(host, e))?;
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
    function: MatrixFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    let y = collect(host, 0)?.ok_or(FunctionFailure::Spreadsheet(VALUE_ERROR))?;
    let x = if length > 1 { collect(host, 1)? } else { None };
    let prediction = matches!(function, MatrixFunction::Growth | MatrixFunction::Trend);
    let nx = if prediction && length > 2 {
        collect(host, 2)?
    } else {
        None
    };
    let constant = option(host, if prediction { 3 } else { 2 }, true)?;
    let stats = if prediction {
        false
    } else {
        option(host, 3, false)?
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
        Some(x) => predictors(&y, x).map_err(|e| matrices::failure(host, e))?,
        None => kernel::sequence(n, 1, 1.0, 1.0, UNLIMITED)
            .map_err(|error| matrices::failure(host, error))?,
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
                            .map_err(|e| matrices::failure(host, e))?,
                    )
                } else if y.rows() == 1 {
                    if nx.rows() != p {
                        return Err(CellError::Ref.into());
                    }
                    output_shape = (1, nx.cols());
                    Some(
                        kernel::transpose(&nx, UNLIMITED)
                            .map_err(|e| matrices::failure(host, e))?,
                    )
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
    let logarithmic = matches!(function, MatrixFunction::Growth | MatrixFunction::Logest);
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
    .map_err(|e| matrices::failure(host, e))?;
    if prediction {
        let predicted = if function == MatrixFunction::Trend {
            fit.predict_trend(nx.as_ref().unwrap_or(&x), UNLIMITED)
        } else {
            fit.predict(nx.as_ref().unwrap_or(&x), UNLIMITED)
        }
        .map_err(|e| matrices::failure(host, e))?;
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
        .map_err(|e| matrices::failure(host, e))?
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
            .map_err(|e| matrices::failure(host, e))?;
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

/// Run modern fit and prediction functions with admitted scratch and uncatchable host failures.
pub(super) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let length = args.len();
    if !(1..=4).contains(&length) {
        return Ok(err(VALUE_ERROR));
    }
    let arrays = if matches!(function, MatrixFunction::Growth | MatrixFunction::Trend) {
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
