use crate::eval::functions::extensions::{ExtensionIdentity, ExtensionTrace};
use crate::eval::limits::EvaluationOutcome;

/// Calculation outcome together with the exact registry identity and executed extension calls.
/// An aborted outcome can retain a partial invocation trace.
#[derive(Clone, Debug)]
pub struct ExtensionEvaluation<T> {
    /// Completed result or operational abort with usage.
    pub outcome: EvaluationOutcome<T>,
    /// Registry identity used to bind and execute this run.
    pub registry_identity: ExtensionIdentity,
    /// Actual provider invocations, possibly partial after an abort.
    pub trace: ExtensionTrace,
}
