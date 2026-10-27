//! Budgeted snapshot decoding: limits are charged before collection reservations
//! and owned string copies.
//!
//! Depends on: no other eval folder; parent artifact (snapshot errors and resources).

/// Cumulative decode budget for values, payload bytes and nesting.
mod budget;
/// Deserializer charging each decoded value against the budget.
mod decoder;

pub(in crate::eval::artifact) use budget::Budget;
pub(in crate::eval::artifact) use decoder::Decoder;
