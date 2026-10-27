//! PIVOTBY connects workbook reads to independent grouping and reduction kernels.
//! Supports one value column and one built-in reducer, with field_headers 0/1 or
//! single-level automatic detection. Field-header output, multiple values,
//! reducer vectors and callable values report UnsupportedCapability.
//! Reducer names come from the shared BuiltinAggregator::from_name mapping;
//! unrecognized names retain the shared resolver's #NAME? outcome.
//! Contract: <https://support.microsoft.com/en-us/excel/functions/pivotby-function>
use super::grouping::{
    aggregate_value, append, generated_text, normalized, option, ratio, reserve,
};
use super::pivot_axis::PivotAxis;
use super::selection::compare;
use crate::eval::adapters::elementwise::{WholeCollection, controls::IntegerControl, matrices};
use crate::eval::host::{Aggregators, CallHost, OperandKind, OperandReader, ScalarOperand};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::matrix::{Budget, FieldRelationship, Matrix};
use poirot_functions::statistics::{BuiltinAggregator, PivotMembers, PivotRelativeTo};

/// Builds a two-axis summary from workbook values within the evaluation budget.
struct PivotExecutor<'h, 'call, 'workbook> {
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> PivotExecutor<'h, 'call, 'workbook> {
    /// Borrow the host that reads arguments, tracks dependencies and checks budgets.
    fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Distinguish invalid header selectors from valid, unsupported presentation modes.
    fn headers(&mut self, rows: usize, fields: usize) -> FunctionResult<bool> {
        let (consume, show) = match option(self.host, 4)? {
            Some(0) => (false, false),
            Some(1) => (true, false),
            Some(2) => (false, true),
            Some(3) => (true, true),
            None => {
                let first = OperandReader::new(self.host).read_cell(2, 0, 0)?.value;
                let consume = rows > 1
                    && first.text_ref().is_some()
                    && matches!(
                        OperandReader::new(self.host).read_cell(2, 1, 0)?.value,
                        CellValue::Number(_)
                    );
                (consume, fields > 1)
            }
            _ => return Err(CellError::Value.into()),
        };
        if show {
            return Err(self.host.unsupported(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::PivotFieldHeaderPublication,
            ));
        }
        Ok(consume)
    }

    /// Collect grouping keys and check the budget before building an axis hierarchy.
    fn axis(
        &mut self,
        index: usize,
        included: &[usize],
        fields: usize,
    ) -> FunctionResult<PivotAxis> {
        let keys = matrices::collect(
            self.host,
            index,
            included.iter().copied(),
            fields,
            |_, operand| Ok(operand.value),
        )?;
        let count = included
            .len()
            .checked_mul(fields)
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| self.host.size_overflow())?;
        let work = poirot_functions::matrix::selection_work(count, included.len(), fields)
            .ok_or_else(|| self.host.size_overflow())?;
        // Each record appears once at every hierarchy level and in the grand total.
        let members = included
            .len()
            .checked_mul(fields.saturating_add(1))
            .ok_or_else(|| self.host.size_overflow())?;
        self.host
            .check_values(count.saturating_mul(4).saturating_add(members))?;
        self.host
            .charge_work(work.saturating_mul(3).saturating_add(
                members.saturating_mul(included.len().checked_ilog2().unwrap_or(0) as usize + 1),
            ) as u64)?;
        PivotAxis::new(
            keys,
            Budget {
                max_cells: count.max(members),
                max_work: work.saturating_mul(3).saturating_add(count),
            },
        )
        .map_err(|error| matrices::failure(self.host, error))
    }

    /// Apply field or aggregate sorting within the hierarchy and position its totals.
    fn layout(
        &mut self,
        axis: &PivotAxis,
        total_index: usize,
        sort_index: usize,
        totals: &[CellValue],
    ) -> FunctionResult<Vec<usize>> {
        let fields = axis.keys.cols();
        let depth = option(self.host, total_index)?.unwrap_or(fields as i64);
        if depth.unsigned_abs() > fields as u64 {
            return Err(CellError::Value.into());
        }
        let mut sorts = reserve(self.host, fields)?;
        if self.host.len() > sort_index && self.host.kind(sort_index)? != OperandKind::Missing {
            let shape = OperandReader::new(self.host).describe(sort_index)?;
            if shape.sheets != 1 || (shape.rows != 1 && shape.columns != 1) {
                return Err(CellError::Value.into());
            }
            let selectors = WholeCollection::at(sort_index).integers(
                self.host,
                IntegerControl::Exact,
                |n, count| {
                    if n == 0
                        || n.unsigned_abs() > fields.saturating_add(1) as u64
                        || count > 1 && n.unsigned_abs() > fields as u64
                    {
                        return Err(CellError::Value.into());
                    }
                    Ok(())
                },
            )?;
            for selector in selectors.into_values() {
                append(&mut sorts, selector)?;
            }
        } else {
            for field in 1..=fields {
                append(&mut sorts, field as i64)?;
            }
        }
        let comparison = |left: usize, right: usize| {
            for &sort in &sorts {
                // SAFETY: Every selector is nonzero by the local validation or positive default loop above.
                let index = sort.unsigned_abs() as usize - 1;
                let pair = if index < fields {
                    axis.groups
                        .get(left)
                        .zip(axis.groups.get(right))
                        .and_then(|(a, b)| {
                            if index >= a.depth || index >= b.depth {
                                return None;
                            }
                            let a = *axis.partition.order.get(a.members.start)?;
                            let b = *axis.partition.order.get(b.members.start)?;
                            axis.keys
                                .get(a, index)
                                .ok()
                                .zip(axis.keys.get(b, index).ok())
                        })
                } else {
                    totals.get(left).zip(totals.get(right))
                };
                if let Some((a, b)) = pair {
                    let order = compare(a, b);
                    let order = if sort < 0 { order.reverse() } else { order };
                    if !order.is_eq() {
                        return order;
                    }
                }
            }
            std::cmp::Ordering::Equal
        };
        let cells = axis.groups.len().saturating_mul(2);
        let work =
            poirot_functions::matrix::selection_work(axis.groups.len(), axis.groups.len(), 1)
                .ok_or_else(|| self.host.size_overflow())?;
        self.host.check_values(cells)?;
        self.host
            .charge_work(work.saturating_mul(sorts.len().max(1)) as u64)?;
        poirot_functions::matrix::group_layout(
            &axis.groups,
            fields,
            FieldRelationship::Hierarchy,
            depth,
            comparison,
            Budget {
                max_cells: cells,
                max_work: work,
            },
        )
        .map_err(|error| matrices::failure(self.host, error))
    }

    /// Reduce selected records with GROUPBY's budget checks and error handling.
    fn reduce(
        &mut self,
        values: &Matrix<ScalarOperand>,
        members: &[usize],
        operation: BuiltinAggregator,
    ) -> FunctionResult<CellValue> {
        let mut operands = reserve(self.host, members.len())?;
        for &row in members {
            append(
                &mut operands,
                values
                    .get(row, 0)
                    .map_err(|error| matrices::failure(self.host, error))?,
            )?;
        }
        aggregate_value(self.host, operation, &operands)
    }

    /// Find records shared by both groups after checking work and result-size budgets.
    fn intersection(
        &mut self,
        left: &PivotMembers,
        right: &PivotMembers,
    ) -> FunctionResult<Vec<usize>> {
        let work = left
            .ordinals()
            .len()
            .checked_add(right.ordinals().len())
            .ok_or_else(|| self.host.size_overflow())?;
        let cells = left.ordinals().len().min(right.ordinals().len());
        self.host.charge_work(work as u64)?;
        self.host.check_values(cells)?;
        left.intersection(
            right,
            Budget {
                max_cells: cells,
                max_work: work,
            },
        )
        .map_err(|error| matrices::failure(self.host, error))
    }

    /// Copy one grouping label with host text accounting or generate a total marker.
    fn label(&mut self, axis: &PivotAxis, index: usize, field: usize) -> FunctionResult<CellValue> {
        let group = axis.groups.get(index).ok_or(CellError::Value)?;
        if field < group.depth {
            let row = *axis
                .partition
                .order
                .get(group.members.start)
                .ok_or(CellError::Value)?;
            let value = axis
                .keys
                .get(row, field)
                .map_err(|error| matrices::failure(self.host, error))?;
            Ok(normalized(self.host.copy_cell_value(value)?))
        } else if field == group.depth {
            generated_text(
                self.host,
                &[crate::model::TextRef::Utf8(if group.depth == 0 {
                    "Total"
                } else {
                    "Subtotal"
                })],
            )
        } else {
            Ok(CellValue::Blank)
        }
    }

    /// Reduce an intersection and, for PERCENTOF, independently reduce its denominator.
    fn cell(
        &mut self,
        axes: (&PivotAxis, &PivotAxis),
        values: &Matrix<ScalarOperand>,
        position: (usize, usize),
        operation: BuiltinAggregator,
        relative: PivotRelativeTo,
    ) -> FunctionResult<CellValue> {
        let (rows, columns) = axes;
        let (row, column) = position;
        let members = self.intersection(
            rows.members.get(row).ok_or(CellError::Value)?,
            columns.members.get(column).ok_or(CellError::Value)?,
        )?;
        if members.is_empty() {
            // An absent intersection does not invoke the reducer, including PERCENTOF.
            return generated_text(self.host, &[]);
        }
        let value = self.reduce(values, &members, operation)?;
        if operation != BuiltinAggregator::PercentOf {
            return Ok(value);
        }
        let (r, c) = match relative {
            PivotRelativeTo::Column => (0, column),
            PivotRelativeTo::Row => (row, 0),
            PivotRelativeTo::Grand => (0, 0),
            PivotRelativeTo::ParentColumn => (
                row,
                columns
                    .groups
                    .get(column)
                    .and_then(|g| g.parent)
                    .unwrap_or(0),
            ),
            PivotRelativeTo::ParentRow => (
                rows.groups.get(row).and_then(|g| g.parent).unwrap_or(0),
                column,
            ),
        };
        let denominator = self.intersection(
            rows.members.get(r).ok_or(CellError::Value)?,
            columns.members.get(c).ok_or(CellError::Value)?,
        )?;
        Ok(ratio(
            value,
            self.reduce(values, &denominator, BuiltinAggregator::Sum)?,
        ))
    }

    /// Build the cross-tab values and labels, then check the final spill dimensions.
    fn execute(&mut self, address: CellAddress) -> FunctionResult<Value> {
        let row_shape = OperandReader::new(self.host).describe(0)?;
        let col_shape = OperandReader::new(self.host).describe(1)?;
        let value_shape = OperandReader::new(self.host).describe(2)?;
        if row_shape.sheets != 1
            || col_shape.sheets != 1
            || value_shape.sheets != 1
            || row_shape.rows != col_shape.rows
            || row_shape.rows != value_shape.rows
        {
            return Err(CellError::Value.into());
        }
        let aggregators = Aggregators::resolve(self.host, 3)?;
        if aggregators.members.len() != 1 || value_shape.columns != 1 {
            return Err(self.host.unsupported(
                CapabilityId::FunctionAdapter,
                crate::eval::limits::EvaluationReasonError::PivotMultipleValuesOrReducers,
            ));
        }
        let operation = *aggregators.members.first().ok_or(CellError::Value)?;
        let relative = match option(self.host, 10)?.unwrap_or(0) {
            0 => PivotRelativeTo::Column,
            1 => PivotRelativeTo::Row,
            2 => PivotRelativeTo::Grand,
            3 => PivotRelativeTo::ParentColumn,
            4 => PivotRelativeTo::ParentRow,
            _ => return Err(CellError::Value.into()),
        };
        let consume = self.headers(row_shape.rows, row_shape.columns.max(col_shape.columns))?;
        let included = WholeCollection::at(9).selected_rows(
            self.host,
            row_shape.rows,
            usize::from(consume),
        )?;
        if included.is_empty() {
            return Err(CellError::Calc.into());
        }
        let rows = self.axis(0, &included, row_shape.columns)?;
        let columns = self.axis(1, &included, col_shape.columns)?;
        let values = matrices::collect(self.host, 2, included.iter().copied(), 1, |_, operand| {
            Ok(operand)
        })?;
        let mut row_totals = reserve(self.host, rows.groups.len())?;
        for row in 0..rows.groups.len() {
            append(
                &mut row_totals,
                self.cell((&rows, &columns), &values, (row, 0), operation, relative)?,
            )?;
        }
        let mut col_totals = reserve(self.host, columns.groups.len())?;
        for column in 0..columns.groups.len() {
            append(
                &mut col_totals,
                self.cell((&rows, &columns), &values, (0, column), operation, relative)?,
            )?;
        }
        let row_layout = self.layout(&rows, 5, 6, &row_totals)?;
        let col_layout = self.layout(&columns, 7, 8, &col_totals)?;
        let output_rows = row_layout
            .len()
            .checked_add(columns.keys.cols())
            .ok_or_else(|| self.host.size_overflow())?;
        let output_cols = col_layout
            .len()
            .checked_add(rows.keys.cols())
            .ok_or_else(|| self.host.size_overflow())?;
        if output_rows > crate::model::MAX_ROWS as usize
            || output_cols > crate::model::MAX_COLUMNS as usize
        {
            return Err(CellError::Num.into());
        }
        let count = output_rows
            .checked_mul(output_cols)
            .ok_or_else(|| self.host.size_overflow())?;
        let mut output = reserve(self.host, count)?;
        for field in 0..columns.keys.cols() {
            for _ in 0..rows.keys.cols() {
                append(&mut output, CellValue::Blank)?;
            }
            for &column in &col_layout {
                append(&mut output, self.label(&columns, column, field)?)?;
            }
        }
        for &row in &row_layout {
            for field in 0..rows.keys.cols() {
                append(&mut output, self.label(&rows, row, field)?)?;
            }
            for &column in &col_layout {
                let value = self.cell(
                    (&rows, &columns),
                    &values,
                    (row, column),
                    operation,
                    relative,
                )?;
                append(&mut output, value)?;
            }
        }
        matrices::array(
            self.host,
            output_rows,
            output_cols,
            output.into_iter(),
            address,
        )
    }
}

/// Execute PIVOTBY as a spill array and report unsupported modes as operational failures.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !(4..=11).contains(&args.len()) {
        return Ok(err(CellError::Value));
    }
    let mut host = CallHost::new_array_slots(session, sheet, address, args, &[0, 1, 2, 6, 8, 9]);
    if args
        .get(3)
        .ok_or_else(|| host.budget_exceeded())?
        .aggregation()
        .map_err(|_| host.budget_exceeded())?
        .is_none()
    {
        return Err(host.capability_failure(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::PivotCallableReducer,
        ));
    }
    match PivotExecutor::new(&mut host).execute(address) {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}
