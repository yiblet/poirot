//! Call-local rectangular branch selection shared by logical execution and the host.
use crate::eval::CellError;

/// A pending, selected, or failed coordinate in a rectangular branch result.
#[derive(Clone, Copy)]
pub(in crate::eval) enum Selection {
    Pending,
    Branch(usize),
    Error(CellError),
}

use crate::eval::array_operator::Shape;
use crate::eval::*;

/// One broadcasting rule applies equally to needed conditions and selected results.
pub(in crate::eval) fn broadcast(left: Shape, right: Shape) -> Option<Shape> {
    fn axis(a: usize, b: usize) -> Option<usize> {
        (a == b || a == 1 || b == 1).then_some(a.max(b))
    }
    Some(Shape {
        rows: axis(left.rows, right.rows)?,
        columns: axis(left.columns, right.columns)?,
    })
}

impl Session<'_> {
    /// Expand selected coordinates when a later condition or result broadcasts.
    pub(in crate::eval) fn expand_selection(
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
        crate::eval::resource::reserve_exact(
            &mut expanded,
            count,
            self.consumer,
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
        self.budget.allocation(arguments.len())?;
        let mut branches = Vec::new();
        branches
            .try_reserve_exact(arguments.len())
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::eval::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "selected branch allocation failed".into(),
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
        assert!(matches!(result, Err(EvaluationFailure::Resource {
            kind: crate::eval::ResourceFailureKind::SizeOverflow,
            capability: CapabilityId::FunctionAdapter,
            source: Some(source), ..
        }) if source == target));
        assert_eq!(session.budget.usage.materialized_values, count as u64);
    }
}
