//! Deferred reference-preserving execution of the pinned lookup family.
//!
//! Depends on: host (CallHost), limits (budgets) and values (value channels and
//! references).

/// Lookup family host entry and R1C1 conversion.
mod dispatch;
/// Connects workbook-independent lookup algorithms to the evaluator's lazy arguments and reads.
mod port;

pub(in crate::eval) use dispatch::execute_host;
