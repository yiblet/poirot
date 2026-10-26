//! Logical scalar kernels, reductions, and deferred branch selection.
mod aggregate;
/// Evaluator-owned logical builtin dispatch.
pub(crate) mod execution;
mod manual;
mod modern;
