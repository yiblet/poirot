//! Compatibility exports for the shared pinned POI numeric primitives.
/// Shared numeric parsing, comparison, and formatting used by database predicates.
pub use crate::numeric::compare;

pub use crate::coercion::compatibility::coercev2_number_to_text;
