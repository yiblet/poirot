//! Scalar coercion, comparison, and Unicode text projection with an operational
//! failure boundary. UTF16 unit consumers must use `text_ref` directly rather than
//! the scalar view.
//!
//! Depends on: limits (budgets).

/// Free scalar coercion, comparison, and text projection functions.
mod scalar;

#[cfg(test)]
mod tests;

pub(in crate::eval) use scalar::{
    checked_text_view, coerce_arithmetic_number, coerce_formula_text, coerce_logical_boolean,
    coerce_operand_boolean, compare, format_failure, semantic, source_compare,
};
