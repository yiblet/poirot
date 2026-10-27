//! Selected-branch evaluation and broadcast expansion for rectangular selections.

use super::rectangular_traversal::RectangularTraversal;
use super::selection::{Selection, broadcast};
use super::shape::Shape;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::session::ObservationRecorder;
use crate::eval::session::Session;
use crate::eval::values::{VALUE_ERROR, Value, err};
use crate::eval::vm::arguments_ref::ArgumentsRef;
use crate::model::CellAddress;
use crate::model::CellError::NA;
use crate::model::CellValue::Error;

/// Evaluates selected branches once and expands selections as their shapes broadcast.
pub(in crate::eval) struct BranchSelector<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> BranchSelector<'s, 'a> {
    /// Borrow the session whose arguments and budget selection uses.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Expand selected coordinates when a later condition or result broadcasts.
    pub(in crate::eval) fn expand(
        &mut self,
        selected: Vec<Selection>,
        previous: Shape,
        output: Shape,
    ) -> EvalResult<Vec<Selection>> {
        if previous.rows == output.rows && previous.columns == output.columns {
            return Ok(selected);
        }
        let count = output
            .rows
            .checked_mul(output.columns)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.session.state.runtime.budget.allocation(count)?;
        let mut expanded = Vec::new();
        crate::eval::limits::reserve_exact(
            &mut expanded,
            count,
            self.session.state.runtime.stack.consumer,
            CapabilityId::FunctionAdapter,
            "selection expansion allocation",
        )?;
        for row in 0..output.rows {
            for column in 0..output.columns {
                let old_row = if previous.rows == 1 { 0 } else { row };
                let old_column = if previous.columns == 1 { 0 } else { column };
                let index = old_row
                    .checked_mul(previous.columns)
                    .and_then(|index| index.checked_add(old_column))
                    .ok_or(EvaluationFailure::StalePlan)?;
                // SAFETY: count is the checked row/column product, reserved above; each coordinate pushes once.
                expanded.push(*selected.get(index).ok_or(EvaluationFailure::StalePlan)?);
            }
        }
        Ok(expanded)
    }

    /// Evaluate selected branch ordinals once and assemble their broadcast result.
    pub(in crate::eval) fn selected_array_results(
        &mut self,
        sheet: usize,
        at: CellAddress,
        arguments: ArgumentsRef<'_>,
        mut selected: Vec<Selection>,
        mut output: Shape,
        result_ordinals: impl Iterator<Item = usize>,
    ) -> EvalResult<Value> {
        // Evaluate only branches selected somewhere, once each. Their singleton axes
        // broadcast exactly like conditions; referenced members remain deferred.
        self.session
            .state
            .runtime
            .budget
            .allocation(arguments.len())?;
        let mut branches = Vec::new();
        branches
            .try_reserve_exact(arguments.len())
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: self.session.state.runtime.stack.consumer,
                detail: "selected branch allocation failed".into(),
            })?;
        // SAFETY: Reservation admitted arguments.len() slots, and resize_with fills exactly that many.
        branches.resize_with(arguments.len(), || None);
        for ordinal in result_ordinals {
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::HelperOperation, selected.len() as u64)?;
            if !selected
                .iter()
                .any(|slot| matches!(slot, Selection::Branch(index) if *index == ordinal))
            {
                continue;
            }
            let value = ObservationRecorder::new(self.session)
                .observed_value_argument(
                    sheet,
                    at,
                    arguments.get(ordinal).ok_or(EvaluationFailure::StalePlan)?,
                    ordinal,
                )?
                .1;
            let shape = Shape::inspect(&value).ok_or_else(|| {
                crate::eval::spill::unsupported(
                    sheet,
                    at,
                    "selection requires rectangular selected results",
                )
            })?;
            let Some(expanded) = broadcast(output, shape) else {
                return Ok(err(VALUE_ERROR));
            };
            selected = self.expand(selected, output, expanded)?;
            output = expanded;
            *branches
                .get_mut(ordinal)
                .ok_or(EvaluationFailure::StalePlan)? = Some((value, shape));
        }
        RectangularTraversal::new(self.session)
            .array(at, output, |session, row, column| {
                let index = row
                    .checked_mul(output.columns)
                    .and_then(|index| index.checked_add(column))
                    .ok_or(EvaluationFailure::StalePlan)?;
                match selected.get(index).ok_or(EvaluationFailure::StalePlan)? {
                    Selection::Pending => Ok(Error(NA)),
                    Selection::Error(error) => Ok(Error(*error)),
                    Selection::Branch(ordinal) => {
                        let (value, shape) = branches
                            .get(*ordinal)
                            .and_then(Option::as_ref)
                            .ok_or(EvaluationFailure::StalePlan)?;
                        RectangularTraversal::new(session).element(value, *shape, row, column)
                    }
                }
            })
            .map(Value::Array)
    }
}
