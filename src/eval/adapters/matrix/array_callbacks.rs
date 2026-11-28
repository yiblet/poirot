//! Higher-order array traversal through the budgeted callable and rectangle ports.

use crate::eval::adapters::elementwise::matrices;
use crate::eval::functions::registry::{FunctionId, MatrixFunction};
use crate::eval::host::{CallHost, OperandDescription, OperandReader};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{AreaKind, FunctionError, FunctionResult, Value, err};
use crate::eval::vm::{ArgumentsRef, Shape};
use crate::model::{CellAddress, CellError, CellValue, MAX_COLUMNS, MAX_ROWS};
use poirot_functions::matrix::Axis;
use poirot_functions::scalar::ParamVec;

/// Applies formula-defined callbacks using the matrix family's budgeted operand host.
pub(super) struct ArrayCallbacks<'call, 'workbook> {
    host: CallHost<'call, 'workbook>,
    address: CellAddress,
}

impl<'call, 'workbook> ArrayCallbacks<'call, 'workbook> {
    /// Dispatch worksheet array helpers that invoke a formula-defined callable.
    pub(super) fn execute(
        function: MatrixFunction,
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        args: ArgumentsRef<'call>,
    ) -> EvalResult<Value> {
        let count = args.len();
        if !FunctionId::Matrix(function)
            .descriptor()
            .arity
            .accepts(count)
        {
            return Ok(err(CellError::Value));
        }
        let mut executor = Self {
            host: CallHost::new_array(session, sheet, address, args, count.saturating_sub(1)),
            address,
        };
        let result = match function {
            MatrixFunction::Map => executor.map(count.saturating_sub(1)),
            MatrixFunction::Byrow => executor.by_axis(Axis::Rows),
            MatrixFunction::Bycol => executor.by_axis(Axis::Columns),
            MatrixFunction::Makearray => executor.make_array(),
            MatrixFunction::Reduce => executor.reduce(),
            MatrixFunction::Scan => executor.scan(),
            _ => return Err(crate::eval::limits::EvaluationError::StalePlan),
        };
        match result {
            Ok(value) => Ok(value),
            Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
            Err(FunctionError::Operational(error)) => Err(error),
        }
    }

    /// Invoke one callback per shared position and fill the rest of the result with #N/A.
    fn map(&mut self, inputs: usize) -> FunctionResult<Value> {
        let host = &mut self.host;
        let at = self.address;
        let callback = host.callback(inputs, inputs, CellError::Value)?;
        let first = Self::rectangle(host, 0)?;
        let (mut rows, mut columns) = (first.rows, first.columns);
        let (mut shared_rows, mut shared_columns) = (rows, columns);
        for index in 1..inputs {
            let next = Self::rectangle(host, index)?;
            rows = rows.max(next.rows);
            columns = columns.max(next.columns);
            shared_rows = shared_rows.min(next.rows);
            shared_columns = shared_columns.min(next.columns);
        }
        host.check_values(inputs)?;
        let mut values = ParamVec::new();
        values
            .try_reserve_exact(inputs)
            .map_err(|source| host.small_vector_reservation_failure(source))?;
        let mut produce = |host: &mut CallHost<'_, '_>, row, column| {
            for index in 0..inputs {
                let operand = Self::callback_member(host, index, row, column)?;
                // SAFETY: The reservation admits inputs entries and the prior position drained the buffer.
                values.push(operand);
            }
            Self::callback_scalar(host, &callback, values.drain(..))
        };
        if (rows, columns) == (shared_rows, shared_columns) {
            return matrices::generate(host, rows, columns, at, produce);
        }
        matrices::generate(host, rows, columns, at, |host, row, column| {
            if row < shared_rows && column < shared_columns {
                produce(host, row, column)
            } else {
                Ok(CellValue::Error(CellError::NA))
            }
        })
    }

    /// Invoke the callback once per row or column, preserving worksheet reference slices.
    fn by_axis(&mut self, axis: Axis) -> FunctionResult<Value> {
        let host = &mut self.host;
        let at = self.address;
        let callback = host.callback(1, 1, CellError::Calc)?;
        let input = Self::rectangle(host, 0)?;
        let (rows, columns, slice_rows, slice_columns) = match axis {
            Axis::Rows => (input.rows, 1, 1, input.columns),
            Axis::Columns => (1, input.columns, input.rows, 1),
        };
        matrices::generate(host, rows, columns, at, |host, row, column| {
            let (source_rows, source_columns) = match axis {
                Axis::Rows => (row..=row, 0..=slice_columns.saturating_sub(1)),
                Axis::Columns => (0..=slice_rows.saturating_sub(1), column..=column),
            };
            if let Some(slice) = OperandReader::new(host).reference_slice(
                0,
                source_rows,
                source_columns,
                AreaKind::Area,
            )? {
                return Self::callback_scalar(host, &callback, std::iter::once(slice));
            }
            let slice = matrices::generate(host, slice_rows, slice_columns, at, |host, r, c| {
                let (source_row, source_column) = match axis {
                    Axis::Rows => (row, c),
                    Axis::Columns => (r, column),
                };
                Ok(OperandReader::new(host)
                    .read_cell(0, source_row, source_column)?
                    .value)
            })?;
            Self::callback_scalar(host, &callback, std::iter::once(slice))
        })
    }

    /// Generate a sized grid from one-based row and column callback arguments.
    fn make_array(&mut self) -> FunctionResult<Value> {
        let host = &mut self.host;
        let at = self.address;
        let rows = dimension(host.scalar_argument(0)?, MAX_ROWS)?;
        let columns = dimension(host.scalar_argument(1)?, MAX_COLUMNS)?;
        let callback = host.callback(2, 2, CellError::Value)?;
        matrices::generate(host, rows, columns, at, |host, row, column| {
            let arguments = [
                CellValue::Number(row as f64 + 1.0).into(),
                CellValue::Number(column as f64 + 1.0).into(),
            ];
            Self::callback_scalar(host, &callback, arguments.into_iter())
        })
    }

    /// Accumulate in row-major order and return the final value channel.
    fn reduce(&mut self) -> FunctionResult<Value> {
        let (callback, input, mut state) = self.accumulation()?;
        let host = &mut self.host;
        host.charge_work((input.rows as u64).saturating_mul(input.columns as u64))?;
        for row in 0..input.rows {
            for column in 0..input.columns {
                state = Some(Self::step(host, &callback, state, row, column)?);
            }
        }
        state.ok_or_else(|| CellError::Calc.into())
    }

    /// Publish one intermediate per source position while retaining the raw state.
    fn scan(&mut self) -> FunctionResult<Value> {
        let (callback, input, mut state) = self.accumulation()?;
        matrices::generate(
            &mut self.host,
            input.rows,
            input.columns,
            self.address,
            |host, row, column| {
                let next = Self::step(host, &callback, state.take(), row, column)?;
                Self::require_single_result(&next)?;
                let published = host.copy_value(&next)?;
                state = Some(next);
                host.select_legacy_scalar(published).map_err(Into::into)
            },
        )
    }

    /// Resolve the callback, input extent and an optional initial accumulator.
    fn accumulation(&mut self) -> FunctionResult<(Value, OperandDescription, Option<Value>)> {
        let callback = self.host.callback(2, 2, CellError::Value)?;
        let input = Self::rectangle(&mut self.host, 1)?;
        let initial = self.host.argument(0)?.into_value();
        let state = (!matches!(initial, Value::Missing)).then_some(initial);
        Ok((callback, input, state))
    }

    /// Advance one position, using the first member unchanged when the seed is omitted.
    fn step(
        host: &mut CallHost<'_, '_>,
        callback: &Value,
        state: Option<Value>,
        row: usize,
        column: usize,
    ) -> FunctionResult<Value> {
        if let Some(state) = state {
            let value = Self::callback_member(host, 1, row, column)?;
            return host
                .invoke_callback(callback, [state, value].into_iter())
                .map_err(Into::into);
        }
        Ok(OperandReader::new(host)
            .read_cell(1, row, column)?
            .value
            .into())
    }

    /// Pass a worksheet reference to the callback, or read a computed member.
    fn callback_member(
        host: &mut CallHost<'_, '_>,
        index: usize,
        row: usize,
        column: usize,
    ) -> FunctionResult<Value> {
        let mut reader = OperandReader::new(host);
        if let Some(reference) =
            reader.reference_slice(index, row..=row, column..=column, AreaKind::Cell)?
        {
            return Ok(reference);
        }
        Ok(reader.read_cell(index, row, column)?.value.into())
    }

    /// Inspect a single bounded input rectangle without reading its members.
    fn rectangle(host: &mut CallHost<'_, '_>, index: usize) -> FunctionResult<OperandDescription> {
        let input = OperandReader::new(host).describe(index)?;
        if input.missing || input.sheets != 1 || input.rows == 0 || input.columns == 0 {
            return Err(CellError::Value.into());
        }
        Ok(input)
    }

    /// Store a callback's single worksheet value and reject nested multi-cell results.
    fn callback_scalar(
        host: &mut CallHost<'_, '_>,
        callback: &Value,
        arguments: impl ExactSizeIterator<Item = Value>,
    ) -> FunctionResult<CellValue> {
        let value = host.invoke_callback(callback, arguments)?;
        Self::require_single_result(&value)?;
        host.select_legacy_scalar(value).map_err(Into::into)
    }

    /// Reject values that cannot occupy one result cell.
    fn require_single_result(value: &Value) -> FunctionResult<()> {
        if matches!(
            Shape::inspect(value),
            Some(Shape {
                rows: 1,
                columns: 1
            })
        ) && !matches!(value, Value::Missing)
        {
            Ok(())
        } else {
            Err(CellError::Calc.into())
        }
    }
}

/// Admit one positive numeric dimension within the corresponding worksheet axis.
fn dimension(value: CellValue, maximum: u32) -> FunctionResult<usize> {
    match value {
        CellValue::Number(number) if (1.0..=f64::from(maximum)).contains(&number) => {
            Ok(number.trunc() as usize)
        }
        CellValue::Error(error) => Err(error.into()),
        _ => Err(CellError::Value.into()),
    }
}
