//! Mathematical kernels using shared typed scalar and collection execution.
/// Evaluator-owned mathematical builtin dispatch.
pub(in crate::eval) mod execution;
mod modern;
mod radix;
mod series;
