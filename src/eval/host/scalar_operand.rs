use crate::eval::values::OperandOrigin;
use crate::model::CellValue;

/// Selected scalar together with its coercion provenance and omission flag.
pub(in crate::eval) struct ScalarOperand {
    /// Selected scalar value.
    pub(in crate::eval) value: CellValue,
    /// Direct, array, or reference origin.
    pub(in crate::eval) origin: OperandOrigin,
    /// Whether source syntax omitted this scalar.
    pub(in crate::eval) missing: bool,
}
