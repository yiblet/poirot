//! Numeric result selection for spreadsheet evaluation. It is independent
//! of text coercion and text semantics.
//!
//! Depends on: limits (evaluation results).

/// Finite, subnormal, and literal numeric result construction.
mod conversion;

#[cfg(test)]
mod tests;

pub(in crate::eval) use conversion::{
    convert_checked_numeric_result, convert_numeric_literal, convert_numeric_outcome,
};
