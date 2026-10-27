//! Evidence of one prepared formula-cache write.

use super::materialized_scalar_summary::MaterializedScalarSummary;
use crate::eval::coordinates::EvaluationTarget;

/// A formula cache write and the scalar value it stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormulaCacheWriteReceipt {
    /// Formula cell receiving the saved result.
    pub target: EvaluationTarget,

    /// Scalar value preserved by the write.
    pub value: MaterializedScalarSummary,
}

impl FormulaCacheWriteReceipt {
    /// Describe a prepared cache write that preserves its computed scalar.
    pub(in crate::eval) fn new(target: EvaluationTarget, value: &crate::model::CellValue) -> Self {
        let computed = MaterializedScalarSummary::from_value(value);
        Self {
            target,
            value: computed,
        }
    }
}
