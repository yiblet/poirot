//! Shared rectangular lifting for scalar-valued numeric utilities.
use crate::host::{CallHost, NumericPolicy, OperandDescription, ScalarOperand};
use crate::*;

/// Admitted argument geometry and its broadcast output rectangle.
pub(super) struct Lift {
    shapes: poirot_functions_core::ParamVec<OperandDescription>,
    start: usize,
    indices: Option<poirot_functions_core::ParamVec<usize>>,
    rows: usize,
    columns: usize,
}
impl Lift {
    /// Describe a bounded prefix of scalar arguments before reading its members.
    pub(super) fn new(host: &mut CallHost<'_, '_>, count: usize) -> FunctionResult<Self> {
        Self::new_at(host, 0, count)
    }
    /// Describe a selected contiguous group, excluding collection operands.
    /// Member indices are relative to `start`; zero count admits a scalar default.
    /// Every host access checks the original `start + index` argument position.
    pub(super) fn new_at(
        host: &mut CallHost<'_, '_>,
        start: usize,
        count: usize,
    ) -> FunctionResult<Self> {
        let end = start.checked_add(count).ok_or(VALUE_ERROR)?;
        Self::from_arguments(host, start..end, start, None)
    }
    /// Describe explicitly selected scalar slots, excluding fixed collection operands.
    pub(super) fn new_selected(
        host: &mut CallHost<'_, '_>,
        selected: &[usize],
    ) -> FunctionResult<Self> {
        host.check_values(selected.len())?;
        let mut indices = poirot_functions_core::ParamVec::new();
        indices
            .try_reserve(selected.len())
            .map_err(|_| host.allocation_failure())?;
        // SAFETY: selected.len slots were reserved for exactly selected.len indices.
        indices.extend(selected.iter().copied());
        Self::from_arguments(host, selected.iter().copied(), 0, Some(indices))
    }
    /// Build shared geometry, retaining contiguous slots without an index allocation.
    fn from_arguments(
        host: &mut CallHost<'_, '_>,
        mut selected: impl ExactSizeIterator<Item = usize>,
        start: usize,
        indices: Option<poirot_functions_core::ParamVec<usize>>,
    ) -> FunctionResult<Self> {
        let count = selected.len();
        host.check_values(count)?;
        let mut shapes = poirot_functions_core::ParamVec::new();
        shapes
            .try_reserve(count)
            .map_err(|_| host.allocation_failure())?;
        let (mut rows, mut columns) = (1, 1);
        for _ in 0..count {
            let index = selected.next().ok_or(VALUE_ERROR)?;
            let shape = host.describe(index)?;
            if shape.sheets != 1 || shape.rows == 0 || shape.columns == 0 {
                return Err(VALUE_ERROR.into());
            }
            rows = rows.max(shape.rows);
            columns = columns.max(shape.columns);
            // SAFETY: count slots were reserved locally and this loop pushes count shapes.
            shapes.push(shape);
        }
        if rows > MAX_ROWS as usize || columns > MAX_COLUMNS as usize {
            return Err(Num.into());
        }
        host.check_values(
            rows.checked_mul(columns)
                .ok_or_else(|| host.size_overflow())?,
        )?;
        host.check_values(rows)?;
        Ok(Self {
            shapes,
            start,
            indices,
            rows,
            columns,
        })
    }
    /// Return the admitted output dimensions for demand-sensitive adapters.
    pub(super) fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.columns)
    }
    /// Read one argument's corresponding member, replicating singleton axes.
    pub(super) fn member(
        &self,
        host: &mut CallHost<'_, '_>,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<ScalarOperand> {
        let shape = self.shapes.get(index).ok_or(VALUE_ERROR)?;
        if shape.rows != 1 && row >= shape.rows || shape.columns != 1 && column >= shape.columns {
            return Err(NA.into());
        }
        host.read_cell(
            match &self.indices {
                Some(indices) => *indices.get(index).ok_or(VALUE_ERROR)?,
                None => self.start.checked_add(index).ok_or(VALUE_ERROR)?,
            },
            if shape.rows == 1 { 0 } else { row },
            if shape.columns == 1 { 0 } else { column },
        )
    }
    /// Numerically coerce one member, distinguishing omission from an actual blank.
    pub(super) fn number(
        &self,
        host: &mut CallHost<'_, '_>,
        index: usize,
        row: usize,
        column: usize,
        default: Option<f64>,
    ) -> FunctionResult<f64> {
        if index >= self.shapes.len() {
            return default.ok_or(VALUE_ERROR.into());
        }
        let operand = self.member(host, index, row, column)?;
        if operand.missing {
            return default.ok_or(VALUE_ERROR.into());
        }
        host.coercev2_scalar_number(&operand.value, NumericPolicy::PinnedPoi)
    }
    /// Evaluate each output member, preserving operational failures outside cells.
    pub(super) fn evaluate(
        &self,
        host: &mut CallHost<'_, '_>,
        address: CellAddress,
        mut evaluate: impl FnMut(&mut CallHost<'_, '_>, usize, usize) -> FunctionResult<CellValue>,
    ) -> FunctionResult<Value> {
        let mut result = Vec::new();
        result
            .try_reserve_exact(self.rows)
            .map_err(|_| host.allocation_failure())?;
        for row in 0..self.rows {
            let mut values = Vec::new();
            values
                .try_reserve_exact(self.columns)
                .map_err(|_| host.allocation_failure())?;
            for column in 0..self.columns {
                host.charge_work(1)?;
                let value = match evaluate(host, row, column) {
                    Ok(value) => value,
                    Err(FunctionFailure::Spreadsheet(error)) => Error(error),
                    Err(error) => return Err(error),
                };
                // SAFETY: Exactly columns pushes follow the local columns reservation.
                values.push(value);
            }
            // SAFETY: Exactly rows pushes follow the local rows reservation.
            result.push(values);
        }
        if self.rows == 1 && self.columns == 1 {
            return result
                .into_iter()
                .next()
                .and_then(|row| row.into_iter().next())
                .map(Value::Scalar)
                .ok_or(VALUE_ERROR.into());
        }
        Ok(Value::Array(ArrayView::anchored(result, Some(address))))
    }
}
