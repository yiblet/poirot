use crate::model;

/// An error while recalculating a workbook.
#[derive(Debug, thiserror::Error)]
pub enum CalculationError {
    /// Calculation settings or workbook preparation were rejected.
    #[error("could not prepare calculation")]
    Preparation(#[source] crate::eval::overlay::OverlayError),
    /// Calculation stopped before all requested results were available.
    #[error("could not finish calculation")]
    Evaluation(#[source] crate::eval::EvaluationError),
    /// Calculated results could not be applied to the workbook.
    #[error("could not apply calculated results")]
    Model(#[source] model::ModelError),
}
