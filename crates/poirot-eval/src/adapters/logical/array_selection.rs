//! Call-local rectangular selection buffers shared by IFS, SWITCH and CHOOSE.
use crate::array_operator::Shape;
use crate::*;

/// Call-local selection state, never a workbook value or cached value tag.
#[derive(Clone, Copy)]
pub(super) enum Selection {
    Pending,
    Branch(usize),
    Error(CellError),
}

/// One broadcasting rule applies equally to needed conditions and selected results.
pub(super) fn broadcast(left: Shape, right: Shape) -> Option<Shape> {
    fn axis(a: usize, b: usize) -> Option<usize> {
        (a == b || a == 1 || b == 1).then_some(a.max(b))
    }
    Some(Shape {
        rows: axis(left.rows, right.rows)?,
        columns: axis(left.columns, right.columns)?,
    })
}

impl Session<'_> {
    pub(super) fn expand_selection(
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
        self.budget.allocation(count)?;
        let mut expanded = Vec::new();
        expanded
            .try_reserve_exact(count)
            .map_err(|_| EvaluationFailure::ResourceLimit {
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "selection expansion allocation failed".into(),
            })?;
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

    pub(super) fn selected_array_results(
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
        self.budget.allocation(arguments.len())?;
        let mut branches = Vec::new();
        branches.try_reserve_exact(arguments.len()).map_err(|_| {
            EvaluationFailure::ResourceLimit {
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "selected branch allocation failed".into(),
            }
        })?;
        // SAFETY: Reservation admitted arguments.len() slots, and resize_with fills exactly that many.
        branches.resize_with(arguments.len(), || None);
        for ordinal in result_ordinals {
            self.budget
                .charge(WorkKind::HelperOperation, selected.len() as u64)?;
            if !selected
                .iter()
                .any(|slot| matches!(slot, Selection::Branch(index) if *index == ordinal))
            {
                continue;
            }
            let value = self
                .observed_value_argument(
                    sheet,
                    at,
                    arguments.get(ordinal).ok_or(EvaluationFailure::StalePlan)?,
                    ordinal,
                )?
                .1;
            let shape = Shape::inspect(&value).ok_or_else(|| {
                self.spill_unsupported(sheet, at, "selection requires rectangular selected results")
            })?;
            let Some(expanded) = broadcast(output, shape) else {
                return Ok(err(VALUE_ERROR));
            };
            selected = self.expand_selection(selected, output, expanded)?;
            output = expanded;
            *branches
                .get_mut(ordinal)
                .ok_or(EvaluationFailure::StalePlan)? = Some((value, shape));
        }
        self.rectangular_array(at, output, |session, row, column| {
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
                    session.rectangular_element(value, *shape, row, column)
                }
            }
        })
        .map(Value::Array)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_capacity_overflow_is_an_attributed_resource_failure() {
        let workbook = Workbook::new();
        let target = EvaluationTarget {
            sheet: 0,
            address: "B2".parse().unwrap(),
        };
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        session.consumer = Some(target);
        session.budget = BudgetMeter::new(EvaluationBudgets {
            run_work: u64::MAX,
            cell_work: u64::MAX,
            materialized_values: usize::MAX,
            ..Default::default()
        })
        .unwrap();
        let count = usize::MAX / std::mem::size_of::<Selection>();
        let result = session.expand_selection(
            vec![Selection::Pending],
            Shape {
                rows: 1,
                columns: 1,
            },
            Shape {
                rows: count,
                columns: 1,
            },
        );
        assert!(matches!(result, Err(EvaluationFailure::ResourceLimit {
            capability: CapabilityId::FunctionAdapter,
            source: Some(source), ..
        }) if source == target));
        assert_eq!(session.budget.usage.materialized_values, count as u64);
    }
}
