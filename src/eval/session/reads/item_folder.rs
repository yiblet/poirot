//! Eager operand member traversal in read order.

use super::sparse_reader::SparseReader;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::eval::session;
use crate::eval::session::cells::{CellEvaluator, ResolvedSource};
use crate::eval::session::composition::Session;
use crate::eval::values::{ReferenceAreas, Value};
use crate::model::CellValue;
use crate::model::CellValue::Blank;

/// Folds eager operand members after admitting each complete batch, preserving read order.
pub(in crate::eval) struct ItemFolder<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ItemFolder<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Fold eager members after admitting each complete batch and preserving read order.
    pub(in crate::eval) fn fold_items<T>(
        &mut self,
        operand: crate::eval::host::HostOperand,
        mut state: T,
        mut reserve_storage: impl FnMut(&mut T, usize) -> Result<(), std::collections::TryReserveError>,
        mut consume: impl FnMut(&mut T, crate::eval::host::ScalarOperand) -> EvalResult<()>,
    ) -> EvalResult<T> {
        use crate::eval::host::ScalarOperand;
        use crate::eval::values::OperandOrigin;
        let (value, missing, array_missing) = operand.into_parts();
        let mut length = 0usize;
        // Keep admission and fallible reservation local to each projected growth.
        macro_rules! reserve {
            ($additional:expr) => {{
                let additional = $additional;
                let count =
                    length
                        .checked_add(additional)
                        .ok_or_else(|| EvaluationError::Resource {
                            kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
                            capability: CapabilityId::FunctionAdapter,
                            target: self.session.state.runtime.stack.consumer,
                            reason: crate::eval::limits::EvaluationReasonError::AggregateMemberCountOverflow,
                        })?;
                self.session.state.runtime.budget.check_allocation(count)?;
                self.session
                    .state
                    .runtime
                    .budget
                    .charge(WorkKind::Materialization, additional as u64)?;
                reserve_storage(&mut state, additional).map_err(|_| {
                    EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        target: self.session.state.runtime.stack.consumer,
                        reason: crate::eval::limits::EvaluationReasonError::AggregateMemberAllocationFailed,
                    }
                })?;
                count
            }};
        }
        match value {
            Value::Missing => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value: Blank,
                        origin: OperandOrigin::Direct,
                        missing: true,
                    },
                )?;
            }
            Value::Scalar(value) => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value,
                        origin: OperandOrigin::Direct,
                        missing,
                    },
                )?;
            }
            Value::SelectedScalar(value, origin) => {
                reserve!(1);
                consume(
                    &mut state,
                    ScalarOperand {
                        value,
                        origin,
                        missing,
                    },
                )?;
            }
            Value::Array(rows) => {
                let count = rows
                    .iter()
                    .try_fold(0usize, |count, row| count.checked_add(row.len()))
                    .ok_or_else(|| EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
                        capability: CapabilityId::FunctionAdapter,
                        target: self.session.state.runtime.stack.consumer,
                        reason: crate::eval::limits::EvaluationReasonError::AggregateMemberCountOverflow,
                    })?;
                reserve!(count);
                for (row, values) in rows.into_iter().enumerate() {
                    for (column, value) in values.into_iter().enumerate() {
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ArrayElement,
                                missing: missing
                                    || array_missing
                                        .get(row)
                                        .and_then(|flags| flags.get(column))
                                        .copied()
                                        .unwrap_or(false),
                            },
                        )?;
                    }
                }
            }
            Value::SignedRefs(areas) => {
                for area in areas {
                    let Some(physical) = area.physical() else {
                        continue;
                    };
                    let members = SparseReader::new(self.session).sparse_members(physical)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
            Value::Refs(areas) => {
                for area in areas {
                    let members = SparseReader::new(self.session).sparse_members(area)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
            Value::SheetSpan(areas) => {
                // Sheet spans retain source-sheet order. Their function-specific
                // semantic error precedence is handled by the ordered consumer.
                for area in areas {
                    let members = SparseReader::new(self.session).sparse_members(area)?;
                    length = reserve!(members.len());
                    for member in members {
                        let value = self.sparse_member_value(area.sheet, member)?;
                        consume(
                            &mut state,
                            ScalarOperand {
                                value,
                                origin: OperandOrigin::ReferencedCell,
                                missing: false,
                            },
                        )?;
                    }
                }
            }
        }
        Ok(state)
    }

    /// Visit stored sheet-span members in sheet order until the consumer stops.
    pub(in crate::eval) fn fold_ordered_sheet_span<T>(
        &mut self,
        areas: ReferenceAreas,
        mut state: T,
        mut consume: impl FnMut(
            &mut T,
            crate::eval::host::ScalarOperand,
        ) -> EvalResult<std::ops::ControlFlow<()>>,
    ) -> EvalResult<T> {
        let mut length = 0usize;
        for area in areas {
            let members = SparseReader::new(self.session).sparse_members(area)?;
            length = length.checked_add(members.len()).ok_or_else(|| {
                EvaluationError::Resource {
                    kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
                    capability: CapabilityId::FunctionAdapter,
                    target: self.session.state.runtime.stack.consumer,
                    reason:
                        crate::eval::limits::EvaluationReasonError::AggregateMemberCountOverflow,
                }
            })?;
            self.session.state.runtime.budget.check_allocation(length)?;
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::Materialization, members.len() as u64)?;
            for member in members {
                let value = self.sparse_member_value(area.sheet, member)?;
                let decision = consume(
                    &mut state,
                    crate::eval::host::ScalarOperand {
                        value,
                        origin: crate::eval::values::OperandOrigin::ReferencedCell,
                        missing: false,
                    },
                )?;
                if decision.is_break() {
                    return Ok(state);
                }
            }
        }
        Ok(state)
    }

    /// Collect projected members while reserving the complete batch before its cell reads.
    #[cfg(test)]
    pub(in crate::eval) fn items_map<T>(
        &mut self,
        value: Value,
        mut project: impl FnMut(CellValue, bool) -> T,
    ) -> EvalResult<Vec<T>> {
        let selected_scalar = matches!(value, Value::SelectedScalar(_, _));
        self.fold_items(
            crate::eval::host::HostOperand::from_value(value),
            Vec::new(),
            |values, additional| values.try_reserve(additional),
            |values, member| {
                if values.len() >= values.capacity() {
                    return Err(EvaluationError::StalePlan);
                }
                // SAFETY: The local capacity check proves this one push cannot grow the vector.
                values.push(project(
                    member.value,
                    !selected_scalar && member.origin != crate::eval::values::OperandOrigin::Direct,
                ));
                Ok(())
            },
        )
    }

    /// Read captured authored content while generated-only members resolve their current source.
    fn sparse_member_value(
        &mut self,
        sheet: usize,
        member: session::SparseMember<'a>,
    ) -> EvalResult<CellValue> {
        match member.cell {
            Some(cell) => {
                let source =
                    ResolvedSource::from_cell(self.session, sheet, member.address, Some(cell));
                CellEvaluator::new(self.session).range_cell_from_source(
                    sheet,
                    member.address,
                    source,
                )
            }
            None => CellEvaluator::new(self.session).range_cell(sheet, member.address),
        }
    }

    /// Retain owned sparse members for host traversal contract tests.
    #[cfg(test)]
    pub(in crate::eval) fn items(&mut self, value: Value) -> EvalResult<Vec<(CellValue, bool)>> {
        self.items_map(value, |value, indirect| (value, indirect))
    }
}
