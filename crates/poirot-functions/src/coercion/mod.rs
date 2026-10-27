//! Convert spreadsheet values into function arguments.
//!
//! This crate is for function implementations. A conversion request selects the
//! accepted value types and how to treat missing arguments, text, and cell references.
//!
//! Use [`Input`](crate::coercion::Input) to describe an argument and [`coerce_args`](crate::coercion::coerce_args) to convert it. Requests
//! such as [`DecimalNumber`](crate::coercion::DecimalNumber) and [`ExactText`](crate::coercion::ExactText) state what the function accepts. The caller
//! supplies a [`CoercionContext`](crate::coercion::CoercionContext) to track conversion work and text allocation.
//!
//! Most workbook applications should use [poirot](https://docs.rs/poirot).
//!
//! ```
//! use poirot_functions::coercion::{
//!     CellValue, CoercionContext, DecimalNumber, ExactText, FirstOf, Choice,
//!     Input, coerce_args,
//! };
//! let left = CellValue::Text("2.5".into());
//! let right = CellValue::Number(3.0);
//! type DecimalPair = (DecimalNumber, DecimalNumber);
//! let mut context = CoercionContext::default();
//! let (a, b) = coerce_args::<DecimalPair>(
//!     &[Input::direct(&left), Input::direct(&right)], &mut context,
//! )?;
//! assert_eq!(a + b, 5.5);
//!
//! // Exact text has priority over interpreting numeric-looking text.
//! type Accepted = FirstOf<((ExactText,), (DecimalNumber,))>;
//! let result = coerce_args::<Accepted>(&[Input::direct(&left)], &mut context)?;
//! assert!(matches!(result, Choice::First(_)));
//! # Ok::<(), poirot_functions::coercion::CoercionFailure>(())
//! ```

mod context;
mod input;
mod parsing;
mod request;
mod scalar;

pub use crate::scalar::{CellError, CellValue, Origin, TextRef};
pub use context::{CoercionContext, CoercionFailure, Limits, Mismatch, Resource, Usage};
pub use input::Input;
pub use request::{Choice, Coerce, FirstOf, Signature, coerce, coerce_args};
pub use scalar::{
    ArrayMember, DecimalNumber, Direct, ExactBoolean, ExactError, ExactNumber, ExactText,
    FormulaText, Identity, LogicalBoolean, Optional, Referenced, TextCount,
};

/// Existing argument profiles, kept distinct during migration.
pub mod compatibility;
/// Context-sensitive numeric date/time parsing.
pub mod date_time;
/// Statistical collection and paired-input policies.
pub mod statistics;

/// Date/time argument plans with explicit context inputs.
pub mod dates;

/// Lookup operand policies and scalar vocabulary.
pub mod lookup;

/// Criteria value vocabulary and SUBTOTAL input rules.
pub mod criteria;

/// Origin-sensitive collection member rules.
pub mod collections;

/// Explicit numeric-text function conversions.
pub mod numeric_text;

mod formatted_number;
pub use formatted_number::coerce_formatted_number;

/// Complex-number argument grammars and computed result spelling.
pub mod complex;

/// Explicit BASE numeric domains and DECIMAL unsigned positional text grammar.
pub mod radix;

/// Roman numeral option admission and bounded text decoding.
pub mod roman;

/// Combinations with replacement scalar and integer-domain admission.
pub mod combinations;

/// Power-series scalar and collection admission.
pub mod series;

/// Publish a completed worksheet formula member without changing empty text.
/// Intermediate expressions and physical empty cells must not request this policy.
// TODO(coercion-non-argument): [LOW] Worksheet publication intentionally loses blank identity; invoking it during expression evaluation would change predicates and selection.
pub fn convert_worksheet_formula_result(value: &mut CellValue) {
    if matches!(value, CellValue::Blank) {
        *value = CellValue::Number(0.0);
    }
}
