//! Modern mixed-value array adapters; geometry belongs to the pure matrix crate.
use super::*;
use crate::host::MissingPolicy;
use crate::registry::MatrixFunction as F;
use poirot_functions_matrix::{Axis, Ignore, Matrix, MemberKind, ScanOrder, Transform};

/// Whether input blanks are retained for filtering or normalized for shaping.
#[derive(Clone, Copy)]
pub(super) enum Members {
    Original,
    Normalized,
}

pub(super) fn matrix(
    host: &mut CallHost<'_, '_>,
    index: usize,
    members: Members,
) -> FunctionResult<Matrix<CellValue>> {
    let shape = host.describe(index)?;
    if shape.sheets != 1 {
        return Err(CellError::Value.into());
    }
    let count = shape
        .rows
        .checked_mul(shape.columns)
        .ok_or_else(|| host.budget_exceeded())?;
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.budget_exceeded())?;
    for row in 0..shape.rows {
        for column in 0..shape.columns {
            let value = host.read_cell(index, row, column)?.value;
            values.push(match members {
                Members::Original => value,
                Members::Normalized => {
                    poirot_coercionv2::compatibility::coercev2_array_member(value)
                }
            });
        }
    }
    Matrix::new(shape.rows, shape.columns, values).map_err(|_| CellError::Value.into())
}
fn count(
    host: &mut CallHost<'_, '_>,
    index: usize,
    blank_default: bool,
) -> FunctionResult<Option<i64>> {
    let operand = host.scalar_data(index, MissingPolicy::Blank)?;
    if operand.missing || blank_default && operand.value == Blank {
        return Ok(None);
    }
    let number = host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)?;
    if !number.is_finite() || number.abs() >= i64::MAX as f64 {
        return Err(CellError::Num.into());
    }
    Ok(Some(number.trunc() as i64))
}
fn evaluate(
    host: &mut CallHost<'_, '_>,
    function: F,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    host.check_values(length)?;
    let first = matrix(
        host,
        0,
        if matches!(function, F::Tocol | F::Torow) {
            Members::Original
        } else {
            Members::Normalized
        },
    )?;
    if matches!(function, F::Tocol | F::Torow) {
        return flatten(host, function, length, address, first);
    }
    let original_rows = first.rows();
    let original_columns = first.cols();
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(length)
        .map_err(|_| host.budget_exceeded())?;
    inputs.push(first);
    let mut padding = Error(CellError::NA);
    let transform = match function {
        F::Hstack | F::Vstack => {
            for index in 1..length {
                inputs.push(matrix(host, index, Members::Normalized)?);
            }
            Transform::Stack(if function == F::Hstack {
                Axis::Columns
            } else {
                Axis::Rows
            })
        }
        F::Take | F::Drop => {
            let rows = count(host, 1, false)?;
            let columns = if length > 2 {
                count(host, 2, false)?
            } else {
                None
            };
            if function == F::Take {
                Transform::Take { rows, columns }
            } else {
                Transform::Drop { rows, columns }
            }
        }
        F::Choosecols | F::Chooserows => {
            let mut indices = Vec::new();
            for index in 1..length {
                let input = matrix(host, index, Members::Normalized)?;
                let capacity = indices
                    .len()
                    .checked_add(input.values().len())
                    .ok_or_else(|| host.budget_exceeded())?;
                host.check_values(capacity)?;
                indices
                    .try_reserve(input.values().len())
                    .map_err(|_| host.budget_exceeded())?;
                for value in input.values() {
                    let n = host.coercev2_scalar_number(value, NumericPolicy::PinnedPoi)?;
                    if !n.is_finite() || n.abs() >= i64::MAX as f64 {
                        return Err(CellError::Value.into());
                    }
                    indices.push(n.trunc() as i64);
                }
            }
            Transform::Choose {
                axis: if function == F::Choosecols {
                    Axis::Columns
                } else {
                    Axis::Rows
                },
                indices,
            }
        }
        F::Wrapcols | F::Wraprows => {
            let operand = host.scalar_data(1, MissingPolicy::Blank)?;
            let n = host
                .coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)?
                .trunc();
            if !n.is_finite() || n < 1.0 {
                return Err(CellError::Num.into());
            }
            // The documented single-vector result caps large counts before conversion.
            let count = n.min(original_rows.max(original_columns) as f64) as usize;
            if length > 2 {
                let operand = host.scalar_data(2, MissingPolicy::Blank)?;
                if !operand.missing {
                    padding =
                        poirot_coercionv2::compatibility::coercev2_array_member(operand.value);
                }
            }
            Transform::Wrap {
                axis: if function == F::Wrapcols {
                    Axis::Columns
                } else {
                    Axis::Rows
                },
                count,
            }
        }
        F::Expand => {
            let dimension = |n: Option<i64>, fallback: usize| -> FunctionResult<usize> {
                match n {
                    None => Ok(fallback),
                    Some(n) => usize::try_from(n).map_err(|_| CellError::Value.into()),
                }
            };
            let rows = dimension(count(host, 1, true)?, original_rows)?;
            let columns = if length > 2 {
                dimension(count(host, 2, true)?, original_columns)?
            } else {
                original_columns
            };
            if length > 3 {
                let operand = host.scalar_data(3, MissingPolicy::Blank)?;
                if !operand.missing {
                    padding =
                        poirot_coercionv2::compatibility::coercev2_array_member(operand.value);
                }
            }
            Transform::Expand { rows, columns }
        }
        _ => return Err(CellError::Value.into()),
    };
    let (rows, columns) = transform.shape(&inputs).map_err(|e| failure(host, e))?;
    if rows > poirot_model::MAX_ROWS as usize || columns > poirot_model::MAX_COLUMNS as usize {
        return Err(CellError::Num.into());
    }
    let cells = rows
        .checked_mul(columns)
        .ok_or_else(|| host.budget_exceeded())?;
    host.check_values(cells)?;
    host.check_values(rows)?;
    host.charge_work(cells as u64)?;
    let result = transform
        .apply(
            &inputs,
            &padding,
            poirot_functions_matrix::Budget {
                max_cells: cells,
                max_work: cells,
            },
        )
        .map_err(|e| failure(host, e))?;
    publish(result, address)
}
fn classify(value: &CellValue) -> MemberKind {
    match value {
        Blank => MemberKind::Blank,
        Error(_) => MemberKind::Error,
        _ => MemberKind::Value,
    }
}
fn flatten(
    host: &mut CallHost<'_, '_>,
    function: F,
    length: usize,
    address: CellAddress,
    input: Matrix<CellValue>,
) -> FunctionResult<Value> {
    let ignore_number = if length > 1 {
        let operand = host.scalar_data(1, MissingPolicy::Blank)?;
        host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)?
    } else {
        0.0
    };
    let ignore = match ignore_number.trunc() {
        0.0 => Ignore::None,
        1.0 => Ignore::Blanks,
        2.0 => Ignore::Errors,
        3.0 => Ignore::BlanksAndErrors,
        _ => return Err(CellError::Value.into()),
    };
    let scan = if length > 2 {
        let operand = host.scalar_data(2, MissingPolicy::Blank)?;
        if core_projection::coercev2_operand_boolean(&operand.value)? {
            ScanOrder::Columns
        } else {
            ScanOrder::Rows
        }
    } else {
        ScanOrder::Rows
    };
    let retained = input
        .values()
        .iter()
        .filter(|v| ignore.retains(classify(v)))
        .count();
    let limit = if function == F::Tocol {
        poirot_model::MAX_ROWS
    } else {
        poirot_model::MAX_COLUMNS
    };
    if retained > limit as usize {
        return Err(CellError::Num.into());
    }
    let work = input
        .values()
        .len()
        .checked_add(retained)
        .ok_or_else(|| host.budget_exceeded())?;
    host.check_values(retained)?;
    host.charge_work(work as u64)?;
    let result = poirot_functions_matrix::flatten(
        &input,
        if function == F::Tocol {
            Axis::Columns
        } else {
            Axis::Rows
        },
        scan,
        ignore,
        classify,
        poirot_functions_matrix::Budget {
            max_cells: retained,
            max_work: work,
        },
    )
    .map_err(|e| failure(host, e))?;
    let rows = result.rows();
    let cols = result.cols();
    let values = result
        .into_values()
        .into_iter()
        .map(poirot_coercionv2::compatibility::coercev2_array_member)
        .collect();
    let result = Matrix::new(rows, cols, values).map_err(|e| failure(host, e))?;
    publish(result, address)
}
pub(super) fn publish(result: Matrix<CellValue>, address: CellAddress) -> FunctionResult<Value> {
    if result.cols() == 0 {
        return Err(CellError::Value.into());
    }
    // SAFETY: The preceding local guard rejects a zero chunk width.
    let values = result
        .values()
        .chunks(result.cols())
        .map(|row| row.to_vec())
        .collect();
    Ok(Value::Array(ArrayView::anchored(values, Some(address))))
}
pub(super) fn failure(
    host: &mut CallHost<'_, '_>,
    error: poirot_functions_matrix::Error,
) -> FunctionFailure {
    match error {
        poirot_functions_matrix::Error::Formula(error) => error.into(),
        _ => host.budget_exceeded().into(),
    }
}
/// Execute a typed modern array operation, retaining operational failures outside cell errors.
pub(crate) fn execute(
    function: F,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if matches!(function, F::Randarray | F::Trimrange) {
        return super::random_trim::execute(function, session, sheet, address, args);
    }
    if matches!(function, F::Groupby | F::Percentof) {
        return super::grouping::execute(function, session, sheet, address, args);
    }
    if matches!(function, F::Filter | F::Sort | F::Sortby | F::Unique) {
        return super::selection::execute(function, session, sheet, address, args);
    }
    if function == F::Sequence {
        return super::sequence(session, sheet, address, args);
    }
    let length = args.len();
    let valid = match function {
        F::Hstack | F::Vstack => (1..=255).contains(&length),
        F::Take | F::Drop => (2..=3).contains(&length),
        F::Choosecols | F::Chooserows => (2..=255).contains(&length),
        F::Expand => (2..=4).contains(&length),
        F::Tocol | F::Torow => (1..=3).contains(&length),
        F::Wrapcols | F::Wraprows => (2..=3).contains(&length),
        _ => false,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let arrays = if matches!(
        function,
        F::Hstack | F::Vstack | F::Choosecols | F::Chooserows
    ) {
        length
    } else {
        1
    };
    let mut host = CallHost::new_array(session, sheet, address, args, arrays);
    match evaluate(&mut host, function, length, address) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
