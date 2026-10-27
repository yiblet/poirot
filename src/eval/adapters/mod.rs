//! Evaluator-owned adapters for workbook-independent function algorithms.
//!
//! Each family implements [`crate::eval::adapters::FunctionAdapter`].
//! [`crate::eval::adapters::FunctionAdapter::chain`] composes families through that same
//! interface; [`crate::eval::adapters::builtins`] exposes the complete catalog.
//! Parsing accepts generic function syntax. Binding resolves names through this
//! registry, and actual execution dispatches through these family adapters.
//! A parent collects child registrations once, indexes known names by numeric slot,
//! and stores uncommon names in a hash map. Executors are flattened when composing
//! parents; calls do not search through the child adapters.
//!
//! Arguments remain deferred. [`crate::eval::adapters::ParamVec`] stores up to four arguments
//! inline and spills for larger calls; it never imposes a function arity limit.
//! [`crate::eval::adapters::Input::force`]
//! evaluates one input on demand, retaining its value for subsequent requests in
//! that call. Distinct inputs and later invocations have separate memoization.
//!
//! ```
//! use poirot::eval::{
//!     adapters::{FunctionAdapter, MathAdapter, FinancialAdapter, ParamVec, CommonFunction, FunctionName},
//!     compile::bytecode, EvaluationBudgets, EvaluationInputs, PreparationPolicy,
//!     EvaluationRun, EvaluationTarget, FormulaCompiler, WorkbookView,
//! };
//! use poirot::model::{Workbook, CellAddress, CellValue};
//!
//! let mut workbook = Workbook::new();
//! workbook.add_sheet("Sheet1")?;
//! let policy = PreparationPolicy::default();
//! let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
//! let view = WorkbookView::new(&workbook, &plan, &policy)?;
//! let mut run = EvaluationRun::new(view, EvaluationInputs::default(), EvaluationBudgets::default())?;
//! let adapter = MathAdapter.chain(FinancialAdapter);
//! let argument = bytecode::compile(&poirot_parsing::formula::Expr::Number(5.))?;
//! let arguments: ParamVec<_> = [argument].into_iter().collect();
//! let target = EvaluationTarget { sheet: 0, address: CellAddress::new(0, 0)? };
//! let name = FunctionName::Known(CommonFunction::Fact);
//! assert_eq!(run.call_with_adapter(&adapter, target, &name, arguments)?, CellValue::Number(120.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: host (CallHost), session (reads, coercion and projection), vm (deferred
//! arguments and selection), spill (spill extents), compile (bound arguments), config
//! (policies, calendar and random streams), functions (registry identities),
//! coordinates (cell targets), limits (budgets and copying), values (value channels and
//! references), ports (dependency reads), inputs (effective workbook cells), reuse
//! (retained lookup prefixes) and observe (logical error observation).

mod aggregate;
/// Default adapter over every builtin family and its dispatch entry points.
mod builtin_adapters;
/// Criteria-based aggregation adapters.
pub mod criteria;
/// Criteria-table database adapters.
pub mod database;
/// Date and time execution adapters.
pub mod dates;
mod elementwise;
/// Engineering and complex number execution adapters.
mod engineering;
/// Per-family builtin adapters generated from one registration pattern.
mod families;
/// Cash flow, loan, and monetary formatting adapters.
pub mod financial;
/// Scalar type and error inspection adapters.
pub mod information;
mod interface;
/// Logical tests and lazy branch selection adapters.
pub mod logical;
/// Shared lookup coercion and comparison rules.
pub mod lookup;
/// Mathematical and random number execution adapters.
pub mod math;
/// Matrix algebra and array aggregation adapters.
pub mod matrix;
/// Typed failures shared by builtin adapters.
mod operational_only;
/// Reference construction and selection adapters.
pub mod reference;
mod registry;
/// Lookup and matching execution adapters.
pub mod search;
/// Statistical calculation and aggregation adapters.
pub mod statistics;
#[cfg(test)]
mod tests;
/// Text construction, parsing, and formatting adapters.
pub mod text;

// Common deferred arguments, values, host context, and adapter interface.
pub use interface::{
    CallContext, FunctionAdapter, FunctionInput, FunctionParameters, FunctionValue,
};

/// Indexed adapter composition owning flattened executors.
pub use registry::AdapterRegistry;

// Deferred input protocol and small argument vector.
pub use poirot_functions::scalar::{Input, ParamVec};

// Builtin adapter composition and per-family adapters.
pub(in crate::eval) use builtin_adapters::execute;
pub use builtin_adapters::{BuiltinAdapters, builtins};
pub use families::{
    CriteriaAdapter, DatabaseAdapter, DateAdapter, EngineeringAdapter, FinancialAdapter,
    InformationAdapter, LogicalAdapter, MathAdapter, MatrixAdapter, ReferenceAdapter,
    SearchAdapter, StatisticsAdapter, TextAdapter,
};

// Parsed builtin names and fallback spellings.
pub use poirot_parsing::formula::{CommonFunction, FunctionName};
