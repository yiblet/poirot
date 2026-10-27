use crate::eval::coordinates::EvaluationTarget;
use crate::model::CellValue;

/// One calculated output returned by bulk evaluation, without changing its source cell.
///
/// Unlike a model `Cell`, this carries no authored formula or saved-result state.
/// Its target identifies the output and its optional formatting comes from the
/// effective source, while `value` is the result of the requested calculation.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEvaluation {
    /// Cell whose result was requested.
    pub target: EvaluationTarget,
    /// Computed value, including semantic spreadsheet errors.
    pub value: CellValue,
    /// Optional source-cell presentation, independent of the computed value.
    /// Explicit TEXT() results remain text; this metadata never coerces values.
    pub formatting: Option<crate::model::CellStyle>,
}
