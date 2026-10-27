//! Numeric result selection under the pinned compatibility policy. It is independent
//! of text coercion and text semantics.
//!
//! Depends on: limits (evaluation results).

/// Finite, subnormal, and literal numeric result construction.
mod conversion;

/// Pinned upstream identity of numeric result behavior.
mod numeric_source_binding;

#[cfg(test)]
mod tests;

pub(in crate::eval) use conversion::{
    convert_checked_numeric_result, convert_numeric_literal, convert_numeric_outcome,
};
pub use numeric_source_binding::NumericSourceBinding;
pub(in crate::eval) use numeric_source_binding::source_binding;
