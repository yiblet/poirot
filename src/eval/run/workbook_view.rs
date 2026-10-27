use crate::eval::compile::compiler::{CompiledWorkbook, PreparationPolicy};
use crate::eval::limits::EvaluationFailure;
use crate::model::Workbook;

/// Pairs a workbook with a compiled plan validated against its formula topology and policy.
/// New runs read current literal values through this view while reusing the checked
/// formula preparation.
pub struct WorkbookView<'a> {
    pub(super) workbook: &'a Workbook,
    pub(super) plan: &'a CompiledWorkbook,
}

impl<'a> WorkbookView<'a> {
    /// Validate that the prepared plan still matches workbook topology and policy.
    pub fn new(
        workbook: &'a Workbook,
        plan: &'a CompiledWorkbook,
        policy: &PreparationPolicy,
    ) -> Result<Self, EvaluationFailure> {
        if !plan.matches(workbook, policy) {
            return Err(EvaluationFailure::StalePlan);
        }
        Ok(Self { workbook, plan })
    }
}
