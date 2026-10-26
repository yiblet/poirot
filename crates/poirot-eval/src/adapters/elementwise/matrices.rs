//! Typed rectangle materialization and checked array construction shared by matrix kernels.
use crate::host::{CallHost, ScalarOperand};
use crate::*;
use poirot_functions_matrix::{Budget, Matrix};

/// Whether rearranged values retain blanks or normalize them to worksheet array members.
#[derive(Clone, Copy)]
pub(in crate::adapters) enum Members {
    /// Retain the original value channel, including UTF-16 text and blanks.
    Original,
    /// Normalize blank array members while retaining all text units and errors.
    Normalized,
}
/// Admit output geometry before allocating rows or invoking a producer.
pub(in crate::adapters) fn admit(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    columns: usize,
) -> FunctionResult<Budget> {
    if rows == 0 || columns == 0 {
        return Err(VALUE_ERROR.into());
    }
    if rows > MAX_ROWS as usize || columns > MAX_COLUMNS as usize {
        return Err(Num.into());
    }
    let cells = rows
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    host.check_values(cells)?;
    host.check_values(rows)?;
    host.charge_work(cells as u64)?;
    Ok(Budget {
        max_cells: cells,
        max_work: cells,
    })
}
/// Traverse an admitted rectangle in row order, retaining operand source classification.
pub(in crate::adapters) fn collect<T>(
    host: &mut CallHost<'_, '_>,
    index: usize,
    mut rows: impl ExactSizeIterator<Item = usize>,
    columns: usize,
    mut convert: impl FnMut(&mut CallHost<'_, '_>, ScalarOperand) -> FunctionResult<T>,
) -> FunctionResult<Matrix<T>> {
    let count = rows.len();
    let capacity = count
        .checked_mul(columns)
        .ok_or_else(|| host.size_overflow())?;
    admit(host, count, columns)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| host.allocation_failure())?;
    for _ in 0..count {
        let row = rows.next().ok_or(VALUE_ERROR)?;
        for column in 0..columns {
            let cell = host.read_cell(index, row, column)?;
            let value = convert(host, cell)?;
            // SAFETY: capacity is the locally checked count * columns product, reserved above;
            // the bounded loops append exactly capacity values.
            values.push(value);
        }
    }
    if rows.next().is_some() {
        return Err(VALUE_ERROR.into());
    }
    Matrix::new(count, columns, values).map_err(|error| failure(host, error))
}
/// Materialize one complete argument using a typed conversion without flattening its geometry.
pub(in crate::adapters) fn map<T>(
    host: &mut CallHost<'_, '_>,
    index: usize,
    convert: impl FnMut(&mut CallHost<'_, '_>, ScalarOperand) -> FunctionResult<T>,
) -> FunctionResult<Matrix<T>> {
    let shape = host.describe(index)?;
    if shape.sheets != 1 {
        return Err(VALUE_ERROR.into());
    }
    collect(host, index, 0..shape.rows, shape.columns, convert)
}
/// Materialize mixed worksheet values without projecting or repairing UTF-16 data.
pub(in crate::adapters) fn matrix(
    host: &mut CallHost<'_, '_>,
    index: usize,
    members: Members,
) -> FunctionResult<Matrix<CellValue>> {
    map(host, index, |_, operand| {
        Ok(match members {
            Members::Original => operand.value,
            Members::Normalized => {
                poirot_coercionv2::compatibility::coercev2_array_member(operand.value)
            }
        })
    })
}
/// Materialize numeric observations, propagating source errors without discarding positions.
pub(in crate::adapters) fn numeric(
    host: &mut CallHost<'_, '_>,
    index: usize,
) -> FunctionResult<Matrix<f64>> {
    map(host, index, |_, operand| match operand.value {
        Number(number) => Ok(number),
        Error(error) => Err(error.into()),
        _ => Err(VALUE_ERROR.into()),
    })
}
/// Materialize square numeric matrices with invalid-member precedence over embedded errors.
pub(in crate::adapters) fn square(
    host: &mut CallHost<'_, '_>,
    index: usize,
) -> FunctionResult<Matrix<f64>> {
    let shape = host.describe(index)?;
    let mut admission = poirot_coercionv2::collections::MatrixAdmission::new(
        poirot_coercionv2::collections::MatrixCollection::SquareNumeric,
        shape.rows,
        shape.columns,
    );
    let matrix = map(host, index, |_, operand| {
        admission
            .coercev2_matrix_member((&operand.value).into())
            .map(|value| value.unwrap_or(0.0))
            .map_err(Into::into)
    })?;
    admission.coercev2_finish()?;
    Ok(matrix)
}
/// Construct an output rectangle, reserving every row before evaluating any producer member.
pub(in crate::adapters) fn generate(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    columns: usize,
    address: CellAddress,
    mut produce: impl FnMut(&mut CallHost<'_, '_>, usize, usize) -> FunctionResult<CellValue>,
) -> FunctionResult<Value> {
    admit(host, rows, columns)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(rows)
        .map_err(|_| host.allocation_failure())?;
    for _ in 0..rows {
        let mut row = Vec::new();
        row.try_reserve_exact(columns)
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: rows slots were reserved and this loop appends exactly rows records.
        output.push(row);
    }
    for (r, row) in output.iter_mut().enumerate() {
        for c in 0..columns {
            let value = produce(host, r, c)?;
            // SAFETY: columns slots were reserved per row; exactly columns members are appended.
            row.push(value);
        }
    }
    Ok(Value::Array(ArrayView::anchored(output, Some(address))))
}
/// Assemble an exact row-major iterator through the common checked producer.
pub(in crate::adapters) fn array(
    host: &mut CallHost<'_, '_>,
    rows: usize,
    columns: usize,
    mut values: impl Iterator<Item = CellValue>,
    address: CellAddress,
) -> FunctionResult<Value> {
    let output = generate(host, rows, columns, address, |_, _, _| {
        values.next().ok_or_else(|| CellError::Ref.into())
    })?;
    if values.next().is_some() {
        return Err(CellError::Ref.into());
    }
    Ok(output)
}
/// Publish a pure matrix through common geometry admission and row assembly.
pub(in crate::adapters) fn publish<T>(
    host: &mut CallHost<'_, '_>,
    result: Matrix<T>,
    address: CellAddress,
    convert: impl FnMut(T) -> CellValue,
) -> FunctionResult<Value> {
    array(
        host,
        result.rows(),
        result.cols(),
        result.into_values().into_iter().map(convert),
        address,
    )
}
/// Preserve pure-kernel failure categories at the common matrix service boundary.
pub(in crate::adapters) fn failure(
    host: &mut CallHost<'_, '_>,
    error: poirot_functions_matrix::Error,
) -> FunctionFailure {
    match error {
        poirot_functions_matrix::Error::Formula(error) => error.into(),
        poirot_functions_matrix::Error::SizeOverflow => host.size_overflow().into(),
        poirot_functions_matrix::Error::AllocationFailure => host.allocation_failure().into(),
        poirot_functions_matrix::Error::BudgetExceeded => host.budget_exceeded().into(),
        poirot_functions_matrix::Error::SourceUnsupported(detail) => {
            host.unsupported(CapabilityId::FunctionAdapter, detail)
        }
    }
}

/// Fold identically shaped sources in coordinate order after checking every source geometry.
pub(in crate::adapters) fn reduce_aligned<T, M>(
    host: &mut CallHost<'_, '_>,
    mut total: T,
    member: impl Fn() -> M,
    mut term: impl FnMut(&mut CallHost<'_, '_>, &mut M, ScalarOperand) -> FunctionResult<()>,
    mut combine: impl FnMut(&mut T, M),
) -> FunctionResult<T> {
    let first = host.describe(0)?;
    for index in 0..host.len() {
        let shape = host.describe(index)?;
        if shape.sheets != 1
            || shape.missing
            || shape.rows != first.rows
            || shape.columns != first.columns
        {
            return Err(VALUE_ERROR.into());
        }
    }
    let budget = admit(host, first.rows, first.columns)?;
    host.charge_work((budget.max_cells as u64).saturating_mul(host.len() as u64))?;
    for row in 0..first.rows {
        for column in 0..first.columns {
            let mut product = member();
            for index in 0..host.len() {
                let operand = host.read_cell(index, row, column)?;
                term(host, &mut product, operand)?;
            }
            combine(&mut total, product);
        }
    }
    Ok(total)
}

/// Visit stored reference positions when available, or a complete dense rectangle otherwise.
pub(in crate::adapters) fn visit(
    host: &mut CallHost<'_, '_>,
    index: usize,
    mut consume: impl FnMut(&mut CallHost<'_, '_>, usize, usize, ScalarOperand) -> FunctionResult<()>,
) -> FunctionResult<()> {
    let shape = host.describe(index)?;
    if shape.sheets != 1 || shape.missing {
        return Err(VALUE_ERROR.into());
    }
    if let Some(positions) = host.stored_positions(index)? {
        host.charge_work(positions.len() as u64)?;
        for (row, column) in positions {
            let operand = host.read_cell(index, row, column)?;
            consume(host, row, column, operand)?;
        }
    } else {
        let count = shape
            .rows
            .checked_mul(shape.columns)
            .ok_or_else(|| host.size_overflow())?;
        host.charge_work(count as u64)?;
        for row in 0..shape.rows {
            for column in 0..shape.columns {
                let operand = host.read_cell(index, row, column)?;
                consume(host, row, column, operand)?;
            }
        }
    }
    Ok(())
}

/// Convert an already materialized rectangle without changing its coordinates or source-read order.
pub(in crate::adapters) fn convert<T, U>(
    host: &mut CallHost<'_, '_>,
    source: Matrix<T>,
    mut convert: impl FnMut(&mut CallHost<'_, '_>, T) -> FunctionResult<U>,
) -> FunctionResult<Matrix<U>> {
    let (rows, columns) = (source.rows(), source.cols());
    let source = source.into_values();
    let mut values = Vec::new();
    values
        .try_reserve_exact(source.len())
        .map_err(|_| host.allocation_failure())?;
    for value in source {
        let value = convert(host, value)?;
        // SAFETY: the exact source length was reserved; conversion appends one value per source member.
        values.push(value);
    }
    Matrix::new(rows, columns, values).map_err(|error| failure(host, error))
}
