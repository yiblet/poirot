//! Statistical adapters with shared typed scalar and collection execution.
/// Evaluator-owned statistical and aggregation dispatch.
pub(crate) mod execution;

mod aggregate;

mod modern;

mod sample;

mod ets;

mod typed;
