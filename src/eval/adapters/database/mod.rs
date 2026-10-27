//! Lazy workbook reads and selected-record aggregation for database functions.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! limits (budgets), values (value channels and function failures)
//!; sibling elementwise (shared reducers).

/// Preserve host aborts separately from database spreadsheet/domain failures.
mod database_match_error;
/// Database family dispatch and its shared helpers.
mod dispatch;
/// Accumulates database payloads accepted by conditional row selection for domain aggregation.
mod selected;

pub(in crate::eval) use dispatch::execute;
