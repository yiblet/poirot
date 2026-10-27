//! Scalar and reference value semantics shared by the VM, the call host and the
//! builtin adapters: internal value channels, physical and signed reference areas,
//! materialized arrays, scalar coercion and comparison, numeric result construction,
//! pinned approximate arithmetic, wildcard matching, budget-admitted value copies,
//! and the failure channel shared by function handlers.
//!
//! Depends on: coordinates (cell targets) and limits (budgets and evaluation results).

/// Physical worksheet rectangles used for reads and spill geometry.
mod area;

/// Cell-versus-area reference provenance.
mod area_kind;

/// Materialized arrays with their coordinate origin.
mod array_view;

/// Budget-admitted copying of evaluated values.
mod budgeted_value;

/// One calculated output returned by bulk evaluation.
mod cell_evaluation;

/// Scalar coercion, comparison, and text projection.
pub(in crate::eval) mod core_projection;

/// Separation of spreadsheet results from operational aborts.
mod function_failure;

/// Result of a helper operation with semantic and operational failures preserved.
mod function_result;

/// Numeric result classification and compatibility policy.
pub(in crate::eval) mod numeric_outcome;

/// Direct, array or reference provenance of a selected scalar.
mod operand_origin;

/// Inline or owned ordered physical reference areas.
mod reference_areas;

/// Signed logical reference geometry and physical read projection.
pub(in crate::eval) mod signed_reference;

/// Pinned approximate addition, subtraction, and number comparison.
pub(in crate::eval) mod source_arithmetic;

/// Internal value channels and scalar result helpers.
mod value;

/// Metered wildcard matching.
mod wildcard_engine;

pub(in crate::eval) use area::Area;
pub(in crate::eval) use area_kind::AreaKind;
pub(in crate::eval) use array_view::ArrayView;
pub(in crate::eval) use budgeted_value::{copy_cell, copy_value};
pub use cell_evaluation::CellEvaluation;
pub(in crate::eval) use function_failure::{FunctionFailure, function_value, identifier_failure};
pub(in crate::eval) use function_result::FunctionResult;
pub(in crate::eval) use operand_origin::OperandOrigin;
pub(in crate::eval) use reference_areas::ReferenceAreas;
pub(in crate::eval) use signed_reference::SignedArea;
pub(in crate::eval) use value::{VALUE_ERROR, Value, convert_legacy_missing_blank, err, finite};
pub(in crate::eval) use wildcard_engine::wildcard;
