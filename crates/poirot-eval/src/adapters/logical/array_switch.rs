//! First-match SWITCH selection with the established scalar comparison policy.
use super::array_selection::{Selection, broadcast};
use super::policy::{self, EvaluationPort, Failure};
use crate::array_operator::Shape;
use crate::host::{CallHost, NumericPolicy};
use crate::*;

fn scalar(host: &mut CallHost<'_, '_>, ordinal: usize) -> EvalResult<Result<CellValue, CellError>> {
    match policy::switch_scalar(EvaluationPort::scalar(host, ordinal)) {
        Ok(value) => Ok(Ok(value)),
        Err(Failure::Cell(error)) => Ok(Err(error)),
        Err(Failure::Operational(failure)) => Err(failure),
        Err(Failure::UnsupportedPoiError(_)) => Err(EvaluationFailure::StalePlan),
    }
}

/// Keep scalar selection and its reference/omission channel until an inspected
/// expression or needed case actually requests rectangular evaluation.
pub(super) fn execute_switch(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if let Err(error) = policy::SelectionPlan::switch(arguments.len()) {
        return Ok(err(error));
    }
    let mut host = CallHost::new_inheriting(session, sheet, at, arguments);
    let expression_array = match semantic(host.retains_array(0))? {
        Ok(retained) => retained,
        Err(error) => return Ok(err(error)),
    };
    if expression_array {
        let expression = match semantic(host.argument(0))? {
            Ok(operand) => operand.into_value(),
            Err(error) => return Ok(err(error)),
        };
        drop(host);
        return session.array_switch(sheet, at, arguments, expression, 1, None);
    }
    let expression = match scalar(&mut host, 0)? {
        Ok(value) => value,
        Err(error) => return Ok(err(error)),
    };
    let default = arguments.len().is_multiple_of(2).then_some(
        arguments
            .len()
            .checked_sub(1)
            .ok_or(EvaluationFailure::StalePlan)?,
    );
    let end = default.unwrap_or(arguments.len());
    // SAFETY: The literal step 2 is nonzero.
    for ordinal in (1..end).step_by(2) {
        match semantic(host.retains_array(ordinal))? {
            Ok(true) => {
                let case = match semantic(host.argument(ordinal))? {
                    Ok(operand) => operand.into_value(),
                    Err(error) => return Ok(err(error)),
                };
                drop(host);
                return session.array_switch(
                    sheet,
                    at,
                    arguments,
                    expression.into(),
                    ordinal,
                    Some(case),
                );
            }
            Ok(false) => {}
            Err(error) => return Ok(err(error)),
        }
        let case = match scalar(&mut host, ordinal)? {
            Ok(value) => value,
            Err(error) => return Ok(err(error)),
        };
        match EvaluationPort::equal(&mut host, &expression, &case) {
            Ok(true) => {
                let selected = ordinal.checked_add(1).ok_or(EvaluationFailure::StalePlan)?;
                return match semantic(host.argument(selected))? {
                    Ok(operand) => Ok(operand.into_value()),
                    Err(error) => Ok(err(error)),
                };
            }
            Ok(false) | Err(Failure::Cell(_)) => {}
            Err(Failure::Operational(failure)) => return Err(failure),
            Err(Failure::UnsupportedPoiError(_)) => return Err(EvaluationFailure::StalePlan),
        }
    }
    match default {
        Some(ordinal) => match semantic(host.argument(ordinal))? {
            Ok(operand) => Ok(operand.into_value()),
            Err(error) => Ok(err(error)),
        },
        None => Ok(err(NA)),
    }
}

impl Session<'_> {
    fn array_switch(
        &mut self,
        sheet: usize,
        at: CellAddress,
        arguments: ArgumentsRef<'_>,
        expression: Value,
        first: usize,
        mut initial: Option<Value>,
    ) -> EvalResult<Value> {
        let expression_shape = Shape::inspect(&expression).ok_or_else(|| {
            self.spill_unsupported(sheet, at, "SWITCH requires a rectangular expression")
        })?;
        let mut output = expression_shape;
        let count = output
            .rows
            .checked_mul(output.columns)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.budget.allocation(count)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "SWITCH selection allocation failed".into(),
            })?;
        // SAFETY: Reservation admitted count slots, and resize fills exactly that many.
        selected.resize(count, Selection::Pending);
        let default = arguments.len().is_multiple_of(2).then_some(
            arguments
                .len()
                .checked_sub(1)
                .ok_or(EvaluationFailure::StalePlan)?,
        );
        let end = default.unwrap_or(arguments.len());
        // SAFETY: The literal step 2 is nonzero.
        for ordinal in (first..end).step_by(2) {
            self.budget
                .charge(WorkKind::HelperOperation, selected.len() as u64)?;
            if !selected
                .iter()
                .any(|slot| matches!(slot, Selection::Pending))
            {
                break;
            }
            let case = match initial.take() {
                Some(value) => value,
                None => {
                    self.observed_value_argument(
                        sheet,
                        at,
                        arguments.get(ordinal).ok_or(EvaluationFailure::StalePlan)?,
                        ordinal,
                    )?
                    .1
                }
            };
            if matches!(case, Value::Missing) {
                return Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: Some(EvaluationTarget { sheet, address: at }),
                    detail: "pinned MissingArgEval scalar coercion throws a source exception"
                        .into(),
                });
            }
            let shape = Shape::inspect(&case).ok_or_else(|| {
                self.spill_unsupported(sheet, at, "SWITCH requires rectangular cases")
            })?;
            let Some(expanded) = broadcast(output, shape) else {
                return Ok(err(VALUE_ERROR));
            };
            selected = self.expand_selection(selected, output, expanded)?;
            output = expanded;
            for (index, slot) in selected.iter_mut().enumerate() {
                if !matches!(slot, Selection::Pending) {
                    continue;
                }
                let row = index
                    .checked_div(output.columns)
                    .ok_or(EvaluationFailure::StalePlan)?;
                let column = index
                    .checked_rem(output.columns)
                    .ok_or(EvaluationFailure::StalePlan)?;
                let expression =
                    self.rectangular_element(&expression, expression_shape, row, column)?;
                // An erroneous expression is terminal at this position, just as
                // scalar SWITCH stops before resolving its first case.
                if matches!(expression, Error(_)) {
                    *slot = Selection::Error(NA);
                    continue;
                }
                let case = self.rectangular_element(&case, shape, row, column)?;
                // The existing scalar SWITCH contract maps erroneous scalar inputs
                // to #N/A and treats a spreadsheet comparison failure as no match.
                if matches!(case, Error(_)) {
                    *slot = Selection::Error(NA);
                } else if semantic(self.equal_scalars(
                    &expression,
                    &case,
                    NumericPolicy::PinnedPoi,
                ))?
                .unwrap_or(false)
                {
                    *slot = Selection::Branch(
                        ordinal.checked_add(1).ok_or(EvaluationFailure::StalePlan)?,
                    );
                }
            }
        }
        if let Some(default) = default {
            self.budget
                .charge(WorkKind::HelperOperation, selected.len() as u64)?;
            for slot in &mut selected {
                if matches!(slot, Selection::Pending) {
                    *slot = Selection::Branch(default);
                }
            }
        }
        self.selected_array_results(
            sheet,
            at,
            arguments,
            selected,
            output,
            // SAFETY: The literal step 2 is nonzero.
            (2..end).step_by(2).chain(default),
        )
    }
}
