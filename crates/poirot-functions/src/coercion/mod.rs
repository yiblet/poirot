//! Convert spreadsheet values into function arguments.
//!
//! This module is for function implementations. A conversion request selects the
//! accepted value types and how to treat missing arguments, text, and cell references.
//!
//! Use [`Input`](crate::coercion::Input) to describe an argument and [`coerce_args`](crate::coercion::coerce_args) to convert it. Requests
//! such as [`DecimalNumber`](crate::coercion::DecimalNumber) and [`ExactText`](crate::coercion::ExactText) state what the function accepts. The caller
//! supplies a [`CoercionContext`](crate::coercion::CoercionContext) to track conversion work and text allocation.
//!
//! An input is one selected scalar: the host resolves references and array shapes
//! before conversion. Provenance is retained because functions may accept literal
//! text or booleans while ignoring the same values in a referenced collection.
//! Missing syntax is distinct from [`CellValue::Blank`](crate::scalar::CellValue::Blank).
//!
//! Only [`CoercionError::Mismatch`](crate::coercion::CoercionError::Mismatch) permits overload fallback. Worksheet errors,
//! exhausted resources and unsupported text projection are terminal. This context
//! meters conversion only; algorithms with host callbacks and
//! [`crate::text::regex::ExecutionLimits`] require their own execution admission.
//! Formatting ([`crate::format`]) renders values for display and does not select an
//! argument-parsing policy.
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
//! # Ok::<(), poirot_functions::coercion::CoercionError>(())
//! ```

mod coercion_error;
mod coercion_mismatch_error;
mod context;
mod criteria_error;
mod date_time_error;
mod expected_input;
mod input;
mod lookup_value_error;
mod numeric_text_error;
mod numeric_text_parse_error;
mod numeric_text_source_error;
mod parsing;
mod request;
mod scalar;
mod text_coercion_error;

pub use crate::scalar::{CellError, CellValue, Origin, TextRef};
pub use context::{CoercionContext, Limits, Resource, Usage};
pub use date_time_error::DateTimeError;
pub use input::Input;
pub use request::{Choice, Coerce, FirstOf, Signature, coerce, coerce_args};
pub use scalar::{
    ArrayMember, DecimalNumber, Direct, ExactBoolean, ExactError, ExactNumber, ExactText,
    FormulaText, Identity, LogicalBoolean, Optional, Referenced, TextCount,
};

/// Function-specific compatibility profiles with distinct parsing and admission rules.
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

pub use coercion_error::CoercionError;

pub use coercion_mismatch_error::CoercionMismatchError;

pub use text_coercion_error::TextCoercionError;

pub use numeric_text_parse_error::NumericTextParseError;

pub use numeric_text_error::NumericTextError;

pub use numeric_text_source_error::NumericTextSourceError;

pub use lookup_value_error::LookupValueError;

pub use criteria_error::CriteriaError;

pub use expected_input::ExpectedInput;
