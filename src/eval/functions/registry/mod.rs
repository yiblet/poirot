//! Executable builtin registry. Aliases point to a typed family handler.
//! Arity is descriptive: family executors preserve function-specific error ordering.
//!
//! Depends on: no other eval folder.

/// Inclusive argument-count bounds of a function contract.
mod arity;
mod catalog;
/// Criteria-based aggregation and subtotal handlers.
mod criteria_function;
/// Database functions aggregating records selected by a criteria table.
mod database_function;
/// Date and time builtin handlers.
mod date_function;
/// Conservative settings and workbook services a handler may use.
mod dependency_policy;
/// Radix, complex number, and special engineering function handlers.
mod engineering_function;
/// Builtin operand and arity evaluation protocol.
mod evaluation_strategy;
/// Cash flow, loan, and monetary formatting handlers.
mod financial_function;
/// Advertised builtin name, typed handler, arity, evaluation strategy, and dependency flags.
mod function_descriptor;
/// Typed builtin handler identity.
mod function_id;
/// Ordinary and profile-only spelling indexes.
mod function_names;
/// Value inspection, conversion, and error construction handlers.
mod information_function;
/// Logical tests and conditional selection handlers.
mod logical_function;
/// Arithmetic, rounding, trigonometry, and random number handlers.
mod math_function;
/// Matrix algebra and array aggregation handlers.
mod matrix_function;
/// Reference construction, selection, and shape handlers.
mod reference_function;
mod result_rule;
/// Lookup and position matching handlers.
mod search_function;
/// Aggregation, ranking, regression, and distribution handlers.
mod statistics_function;
#[cfg(test)]
mod tests;
/// Text construction, parsing, formatting, and matching handlers.
mod text_function;
/// Legacy lightweight metadata identity.
mod udf_identity;

/// Catalog used for Excel-compatible name resolution.
pub(in crate::eval) use catalog::resolve_handler;

// Advertised builtin catalog and spelling resolver.
pub use catalog::{FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS, resolve_function};

// Mandatory possible-result transfer rules for registered builtins.
pub use result_rule::ResultRule;

// Registration contracts, function identities and builtin family names.
pub use arity::Arity;
pub use criteria_function::CriteriaFunction;
pub use database_function::DatabaseFunction;
pub use date_function::DateFunction;
pub use dependency_policy::DependencyPolicy;
pub use engineering_function::EngineeringFunction;
pub use evaluation_strategy::EvaluationStrategy;
pub use financial_function::FinancialFunction;
pub use function_descriptor::FunctionDescriptor;
pub use function_id::FunctionId;
pub(in crate::eval) use function_names::FunctionNames;
pub use information_function::InformationFunction;
pub use logical_function::LogicalFunction;
pub use math_function::MathFunction;
pub use matrix_function::MatrixFunction;
pub use reference_function::ReferenceFunction;
pub use search_function::SearchFunction;
pub use statistics_function::StatisticsFunction;
pub use text_function::TextFunction;
pub use udf_identity::UdfIdentity;
