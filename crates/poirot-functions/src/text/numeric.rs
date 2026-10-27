//! NUMBERVALUE conversion lives in the central input-policy crate.
pub use crate::coercion::numeric_text::{
    ParseFailure, coerce_number_value, coerce_number_value_excel,
};
