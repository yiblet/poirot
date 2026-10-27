use crate::model::CellValue;

/// Provider result: a scalar, rectangular row-major array, or unchanged argument channel.
/// Array outputs require the arrays capability; final scalar/array handling follows the run API.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionOutput {
    /// Scalar result, including semantic spreadsheet errors.
    Scalar(CellValue),
    /// Rectangular row-major result requiring the arrays capability.
    Array(Vec<Vec<CellValue>>),
    /// Preserve a requested argument's reference/array/missing channel.
    Argument(usize),
}
