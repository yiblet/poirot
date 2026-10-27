//! Formula compilation: formula text to bound expression to bytecode program, plus the
//! possible-result typing computed during preparation.
//!
//! Depends on: functions (registry identities, extension bindings and external names),
//! coordinates (evaluation targets) and limits (preparation budgets).

/// Bound expression representation used while compiling formulas.
pub mod bound;

/// Serializable executable formula instructions and operands.
pub mod bytecode;

/// Formula compilation and immutable execution preparation.
pub mod compiler;

/// Borrowed compilation services, independent of execution state and observers.
mod source_preparation;

/// Possible-result types and composable properties of prepared expressions.
pub mod typing;

pub(in crate::eval) use source_preparation::SourcePreparation;
