//! Lazy workbook reads for the pinned POI DStarRunner database family.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! config (policies), limits (budgets), values (value channels and function failures)
//! and ports (dependency reads); sibling elementwise (shared reducers).

/// Preserve host aborts separately from database spreadsheet/domain failures.
mod database_match_failure;
/// Database family dispatch and its shared helpers.
mod dispatch;
/// Accumulates database payloads accepted by conditional row selection for domain aggregation.
mod selected;

pub(in crate::eval) use dispatch::execute;
