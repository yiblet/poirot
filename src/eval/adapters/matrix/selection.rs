//! Modern selection adapters; pure kernels own permutation and duplicate grouping.
//! Contracts: <https://support.microsoft.com/en-us/excel/functions/filter-function>
//! <https://support.microsoft.com/en-us/excel/functions/sort-function>
//! <https://support.microsoft.com/en-us/excel/functions/sortby-function>
//! <https://support.microsoft.com/en-us/excel/functions/unique-function>
use crate::eval::adapters::elementwise::matrices::{Members, failure, matrix};
use crate::eval::adapters::elementwise::{
    FixedScalar, NumericParameter, WholeCollection, matrices,
};
use crate::eval::functions::registry::MatrixFunction;
use crate::eval::host::CallHost;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::matrix::{Axis, Budget, Matrix, SortOrder};
use std::cmp::Ordering;

fn order(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<SortOrder> {
    match FixedScalar::at(index).number(host, NumericParameter::constant(1.0))? {
        1.0 => Ok(SortOrder::Ascending),
        -1.0 => Ok(SortOrder::Descending),
        _ => Err(CellError::Value.into()),
    }
}

fn axis(by_col: bool) -> Axis {
    if by_col { Axis::Columns } else { Axis::Rows }
}

fn rank(value: &CellValue) -> u8 {
    match value {
        Number(_) => 0,
        Text(_) | CellValue::TextUtf16(_) => 1,
        Boolean(_) => 2,
        Error(_) => 3,
        Blank => 4,
    }
}

pub(super) fn compare(a: &CellValue, b: &CellValue) -> Ordering {
    if let (Some(a), Some(b)) = (a.text_ref(), b.text_ref()) {
        return poirot_functions::text_units::fold::compare_utf16(a.units(), b.units());
    }
    match (a, b) {
        (Number(a), Number(b)) if a == b => Ordering::Equal,
        (Number(a), Number(b)) => a.total_cmp(b),
        (Boolean(a), Boolean(b)) => a.cmp(b),
        (Error(a), Error(b)) => (*a as u8).cmp(&(*b as u8)),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// A sorting member with its key's direction; true blanks always follow ordinary values.
#[derive(Clone)]
struct SortMember {
    value: CellValue,
    order: SortOrder,
}

fn compare_sort(a: &SortMember, b: &SortMember) -> Ordering {
    match (&a.value, &b.value) {
        (Blank, Blank) => Ordering::Equal,
        (Blank, _) => Ordering::Greater,
        (_, Blank) => Ordering::Less,
        _ => {
            let result = compare(&a.value, &b.value);
            if a.order == SortOrder::Descending {
                result.reverse()
            } else {
                result
            }
        }
    }
}

fn admit(
    host: &mut CallHost<'_, '_>,
    input: &Matrix<CellValue>,
    axis: Axis,
    keys: usize,
) -> FunctionResult<Budget> {
    let records = if axis == Axis::Rows {
        input.rows()
    } else {
        input.cols()
    };
    let work = poirot_functions::matrix::selection_work(input.values().len(), records, keys)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(input.values().len())?;
    host.charge_work(work as u64)?;
    Ok(Budget {
        max_cells: input.values().len(),
        max_work: work,
    })
}

fn normalized(
    host: &mut CallHost<'_, '_>,
    result: Matrix<CellValue>,
    address: CellAddress,
) -> FunctionResult<Value> {
    matrices::publish(
        host,
        result,
        address,
        poirot_functions::coercion::compatibility::coerce_array_member,
    )
}

fn evaluate(
    host: &mut CallHost<'_, '_>,
    function: MatrixFunction,
    length: usize,
    address: CellAddress,
) -> FunctionResult<Value> {
    let input = matrix(host, 0, Members::Original)?;
    if function == MatrixFunction::Filter {
        let (axis, include) = WholeCollection::at(1).booleans(host, |rows, columns| {
            if columns == 1 && rows == input.rows() {
                Ok(Axis::Rows)
            } else if rows == 1 && columns == input.cols() {
                Ok(Axis::Columns)
            } else {
                Err(CellError::Value.into())
            }
        })?;
        let mask = include.into_values();
        if !mask.iter().any(|keep| *keep) {
            if length < 3 {
                return Err(CellError::Calc.into());
            }
            return {
                let result = matrix(host, 2, Members::Normalized)?;
                normalized(host, result, address)
            };
        }
        host.charge_work(input.values().len() as u64)?;
        let budget = Budget {
            max_cells: input.values().len(),
            max_work: input.values().len(),
        };
        let result = poirot_functions::matrix::filter(&input, axis, &mask, budget)
            .map_err(|e| failure(host, e))?;
        return normalized(host, result, address);
    }
    if function == MatrixFunction::Unique {
        let axis = axis(FixedScalar::at(1).boolean(host, false)?);
        let once = FixedScalar::at(2).boolean(host, false)?;
        let keys = if axis == Axis::Rows {
            input.cols()
        } else {
            input.rows()
        };
        let budget = admit(host, &input, axis, keys)?;
        let result = poirot_functions::matrix::unique(&input, axis, once, compare, budget)
            .map_err(|e| failure(host, e))?;
        return normalized(host, result, address);
    }
    let mut keys = Vec::new();
    let mut orders = Vec::new();
    let axis = if function == MatrixFunction::Sort {
        let axis = axis(FixedScalar::at(3).boolean(host, false)?);
        let index = FixedScalar::at(1).number(host, NumericParameter::constant(1.0))?;
        let key_limit = if axis == Axis::Rows {
            input.cols()
        } else {
            input.rows()
        };
        if !index.is_finite() || index < 1.0 || index > key_limit as f64 || index.fract() != 0.0 {
            return Err(CellError::Value.into());
        }
        // SAFETY: The preceding local check establishes a finite integral index of at least one.
        let index = index as usize - 1;
        let records = if axis == Axis::Rows {
            input.rows()
        } else {
            input.cols()
        };
        let mut values = Vec::new();
        values
            .try_reserve_exact(records)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        for i in 0..records {
            values.push(
                match axis {
                    Axis::Rows => input.get(i, index),
                    Axis::Columns => input.get(index, i),
                }
                .map_err(|e| failure(host, e))?
                .clone(),
            );
        }
        keys.push(Matrix::new(records, 1, values).map_err(|e| failure(host, e))?);
        orders.push(order(host, 2)?);
        axis
    } else {
        let mut orientation = None;
        for index in (1..length).step_by(2) {
            let key = matrix(host, index, Members::Original)?;
            let current = if key.cols() == 1 && key.rows() == input.rows() {
                Axis::Rows
            } else if key.rows() == 1 && key.cols() == input.cols() {
                Axis::Columns
            } else {
                return Err(CellError::Value.into());
            };
            if orientation.is_some_and(|a| a != current) {
                return Err(CellError::Value.into());
            }
            orientation = Some(current);
            orders.push(order(host, index + 1)?);
            keys.push(key);
        }
        orientation.ok_or(CellError::Value)?
    };
    let records = if axis == Axis::Rows {
        input.rows()
    } else {
        input.cols()
    };
    let count = records
        .checked_mul(keys.len())
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
    for record in 0..records {
        for (key, order) in keys.iter().zip(&orders) {
            values.push(SortMember {
                value: key.values().get(record).ok_or(CellError::Value)?.clone(),
                order: *order,
            });
        }
    }
    let keys = Matrix::new(records, keys.len(), values).map_err(|e| failure(host, e))?;
    // Direction belongs to SortMember so blank placement is independent of reversal.
    orders.fill(SortOrder::Ascending);
    let mut budget = admit(host, &input, axis, keys.cols())?;
    budget.max_cells = budget.max_cells.max(count);
    let result = poirot_functions::matrix::sort(&input, axis, &keys, &orders, compare_sort, budget)
        .map_err(|e| failure(host, e))?;
    normalized(host, result, address)
}

/// Execute selection calls with deferred operands and operational error separation.
pub(super) fn execute(
    function: MatrixFunction,
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let length = args.len();
    let valid = match function {
        MatrixFunction::Filter => (2..=3).contains(&length),
        MatrixFunction::Sort => (1..=4).contains(&length),
        MatrixFunction::Unique => (1..=3).contains(&length),
        MatrixFunction::Sortby => (2..=255).contains(&length),
        _ => false,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, length);
    match evaluate(&mut host, function, length, address) {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}
