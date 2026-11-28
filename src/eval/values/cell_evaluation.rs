use crate::eval::coordinates::EvaluationTarget;
use crate::model::{Cell, CellValue};

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

impl CellEvaluation {
    /// Attach the formatting of the effective authored cell to a calculated value.
    pub(in crate::eval) fn from_source(
        target: EvaluationTarget,
        value: CellValue,
        source: Option<&Cell>,
    ) -> Self {
        let formatting = source
            .map(|cell| &cell.style)
            .filter(|style| style.number_format.is_some() || style.visual.is_some())
            .cloned();
        Self {
            target,
            value,
            formatting,
        }
    }
}
