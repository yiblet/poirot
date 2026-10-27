//! Statistical adapters with shared typed scalar and collection execution.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! functions (identities), limits (budgets), values (value channels and failures) and
//! ports (dependency reads); sibling elementwise (shared reducers).
/// Evaluator-owned statistical and aggregation dispatch.
pub(in crate::eval) mod execution;

mod aggregate;

mod modern;

mod sample;

mod ets;

mod typed;
