//! Cell values and deferred arguments for spreadsheet functions.
//!
//! [`CellValue`] represents a scalar value, including spreadsheet errors and Excel
//! text. [`Origin`] distinguishes literal arguments from values read through references.
//!
//! Implement [`Input`] to let a function evaluate an argument only when it needs it.
//! These types let function algorithms run without a workbook or formula evaluator.
//! Workbook storage lives in [poirot-model](https://docs.rs/poirot-model).
mod error;
mod excel_text;
mod value;
// Spreadsheet errors and model-validation diagnostics shared across crate boundaries.
pub use error::{CellError, ValueError};
// Exact Excel UTF-16 storage and checked text-unit selection errors.
pub use excel_text::{ExcelText, TextUnitsError};
// Canonical scalar values and borrowed access for either physical text encoding.
pub use value::{CellValue, TextRef, TextUnits};

/// Four arguments stay inline; larger calls spill without an artificial arity limit.
pub type ParamVec<T> = smallvec::SmallVec<[T; 4]>;

/// Deferred input supplied by an adapter. Forcing evaluates only this input.
/// A successful value is memoized for the lifetime of that input within its call;
/// distinct arguments and later invocations must have separate memoization.
/// Implementations retain operational failures separately from scalar errors.
/// A pure algorithm can request only its selected input, without depending on a
/// formula AST, workbook, or execution engine:
/// ```
/// use poirot_functions_core::Input;
/// fn select<I: Input>(condition: bool, yes: &mut I, no: &mut I) -> Result<I::Value, I::Error> {
///     if condition { yes.force() } else { no.force() }
/// }
/// ```
pub trait Input {
    /// Value channel produced when this deferred argument is forced.
    type Value;
    /// Operational failure channel; distinct from spreadsheet error values.
    type Error;
    /// Evaluate this argument on demand, reusing its result within this invocation.
    fn force(&mut self) -> Result<Self::Value, Self::Error>;
}

/// Scalar projection after a host has applied its explicit text/coercion policy.
/// Missing is distinct from an actual blank cell.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    /// Numeric operand admitted by the host.
    Number(f64),
    /// Boolean operand before family-specific coercion.
    Boolean(bool),
    /// Borrowed exact text operand.
    Text(String),
    /// Explicit or referenced blank value.
    Blank,
    /// Omitted argument slot, retained separately from blank.
    Missing,
    /// Spreadsheet error propagated under the family's precedence rules.
    Error(CellError),
}
/// Operand provenance used by family kernels to distinguish direct and referenced coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Literal or computed scalar supplied directly to the call.
    Direct,
    /// Scalar selected from a worksheet reference.
    Reference,
    /// Scalar selected from a materialized array.
    Array,
}
#[derive(Clone, Debug, PartialEq)]
/// A selected scalar retaining direct, reference, or array origin for coercion.
/// Selecting one value does not erase the argument's source category.
pub struct ResolvedScalar {
    /// Selected scalar before family-specific conversion.
    pub value: Scalar,
    /// Provenance that controls reference-versus-literal coercion.
    pub origin: Origin,
}
