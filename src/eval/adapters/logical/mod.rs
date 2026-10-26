//! Logical scalar kernels, reductions, and deferred branch selection.
mod aggregate;
/// Evaluator-owned logical builtin dispatch.
pub(in crate::eval) mod execution;
mod manual;
mod modern;
