//! Lazy first-true selection over conformable rectangular operands.
use super::policy;
use crate::array_operator::Shape;
use crate::*;

use super::array_selection::{Selection, broadcast};

impl Session<'_> {
    pub(super) fn array_ifs(
        &mut self,
        sheet: usize,
        at: CellAddress,
        arguments: ArgumentsRef<'_>,
        first: usize,
        condition: Value,
    ) -> EvalResult<Value> {
        let mut output = Shape {
            rows: 1,
            columns: 1,
        };
        self.budget.allocation(1)?;
        // SAFETY: This literal constructs exactly one fixed-size Selection value.
        let mut selected = vec![Selection::Pending];
        let mut initial = Some(condition);
        // SAFETY: The literal step 2 is nonzero.
        for ordinal in (first..arguments.len()).step_by(2) {
            self.budget
                .charge(WorkKind::HelperOperation, selected.len() as u64)?;
            if !selected
                .iter()
                .any(|slot| matches!(slot, Selection::Pending))
            {
                break;
            }
            let condition = match initial.take() {
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
            if matches!(condition, Value::Missing) {
                return Err(EvaluationFailure::UnsupportedCapability {
                    capability: CapabilityId::FunctionAdapter,
                    source: Some(EvaluationTarget { sheet, address: at }),
                    detail: "pinned MissingArgEval scalar coercion throws a source exception"
                        .into(),
                });
            }
            let shape = Shape::inspect(&condition).ok_or_else(|| {
                self.spill_unsupported(sheet, at, "IFS requires rectangular conditions")
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
                let value = self.rectangular_element(&condition, shape, row, column)?;
                *slot = match policy::coercev2_logical_test(&value) {
                    Ok(true) => Selection::Branch(
                        ordinal.checked_add(1).ok_or(EvaluationFailure::StalePlan)?,
                    ),
                    Ok(false) => Selection::Pending,
                    Err(error) => Selection::Error(error),
                };
            }
        }
        self.selected_array_results(
            sheet,
            at,
            arguments,
            selected,
            output,
            // SAFETY: The literal step 2 is nonzero.
            (first.checked_add(1).ok_or(EvaluationFailure::StalePlan)?..arguments.len()).step_by(2),
        )
    }
}
