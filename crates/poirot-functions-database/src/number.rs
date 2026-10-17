//! Compatibility exports for the shared pinned POI numeric primitives.
/// Shared numeric parsing, comparison, and formatting used by database predicates.
pub use poirot_numeric::{compare, parse_hex};

pub use poirot_coercionv2::compatibility::coercev2_number_to_text;
