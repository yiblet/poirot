//! Positional CHOOSE selection uses the existing scalar index admission policy.
use super::array_selection::Selection;
use crate::array_operator::Shape;
use crate::*;

impl Session<'_> {
    pub(super) fn array_choose(
        &mut self,
        sheet: usize,
        at: CellAddress,
        arguments: ArgumentsRef<'_>,
        indices: Value,
    ) -> EvalResult<Value> {
        let output = Shape::inspect(&indices).ok_or_else(|| {
            self.spill_unsupported(sheet, at, "CHOOSE requires rectangular indices")
        })?;
        let count = output
            .rows
            .checked_mul(output.columns)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.budget.allocation(count)?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| EvaluationFailure::ResourceLimit {
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "CHOOSE selection allocation failed".into(),
            })?;
        for row in 0..output.rows {
            for column in 0..output.columns {
                let value = self.rectangular_element(&indices, output, row, column)?;
                // SAFETY: count is the checked row/column product, reserved above; each coordinate pushes once.
                selected.push(match semantic(self.coercev2_scalar_number(&value))? {
                    Ok(index) if index >= 1. && index.trunc() < arguments.len() as f64 => {
                        Selection::Branch(index as usize)
                    }
                    Ok(_) => Selection::Error(VALUE_ERROR),
                    Err(error) => Selection::Error(error),
                });
            }
        }
        self.selected_array_results(sheet, at, arguments, selected, output, 1..arguments.len())
    }
}
