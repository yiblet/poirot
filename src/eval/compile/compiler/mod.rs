//! Parse and bind formulas, reusing compiled programs independently of their results.
//!
//! ```
//! use poirot::eval::{FormulaCompiler, PreparationPolicy, WorkbookView};
//! use poirot::model::Workbook;
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_formula("A1".parse()?, "2*3")?;
//! let policy = PreparationPolicy::default();
//! let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
//! let _view = WorkbookView::new(&book, &plan, &policy)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: functions (registry and extensions) and coordinates (formula
//! cell targets); siblings bound, bytecode and typing.

/// Serializable compiled-source cache used by overlay snapshots.
pub(in crate::eval) mod cache_snapshot;
/// Formula text of a compiler-cache entry, borrowed when captured and shared when decoded.
mod cached_source;
/// Serialized call metadata of a plan snapshot program.
mod call_dto;
/// Prepared source, reusable bytecode, and possible dependencies shared by plans.
mod compiled_formula;
/// Immutable compiled formula index and owned source topology; contains no cell results.
mod compiled_workbook;
/// Cumulative preparation counters for one [`FormulaCompiler`].
mod compiler_usage;
/// Parses, binds, and interns reusable formula bytecode; stores no evaluated results.
mod formula_compiler;
/// Full compiled-plan DTO for the trusted local disk cache.
mod plan_snapshot;
/// Conservative references and names visible in formula syntax before execution.
mod potential_dependencies;
/// A sheet-qualified defined name found while preparing a formula.
mod potential_qualified_name;
/// A reference present in formula syntax, whether or not execution reads it.
mod potential_reference;
/// Parser limits and function definitions used to compile formulas.
mod preparation_policy;
/// A preparation failure is deferred until its cell or name is actually evaluated.
mod prepared_formula;
/// Serialized shared program of a plan snapshot.
mod program_dto;
/// The function definitions against which formulas are compiled.
mod registry_identity;
/// Advisory locality hint whose admission was checked during the existing key pass.
mod scalar_locality;
/// Recognizes formulas that differ only in point-cell references and are proven not to spill.
mod scalar_matcher;
/// Copies decoded formula text once, directly into a shared allocation.
mod shared_source_visitor;
/// Bounds-checked reads of serialized compiled-program fields.
mod snapshot_reader;
/// Serialized formula source of a plan snapshot.
mod source_dto;
/// In-process source digests used as plan-reuse lookup hints.
mod source_fingerprint;
/// Exact formula, name, sheet, and visibility metadata guarding workbook plan reuse.
mod source_identity;
/// Reuse indexes for one compiler policy: proven non-spilling formula classes, reference-rebinding candidates, and identical executable programs.
mod template_pool;
/// Structural validation of restored bytecode templates and reference bindings.
mod template_validation;
/// Reusable formula templates keyed by normalized executables.
mod templates;
/// Unit tests for formula preparation, template pooling and spill classification.
#[cfg(test)]
mod tests;

pub use compiled_formula::CompiledFormula;
pub use compiled_workbook::CompiledWorkbook;
pub use compiler_usage::CompilerUsage;
pub use formula_compiler::FormulaCompiler;
pub use plan_snapshot::PlanSnapshot;
pub use potential_dependencies::PotentialDependencies;
pub use potential_qualified_name::PotentialQualifiedName;
pub use potential_reference::PotentialReference;
pub use preparation_policy::PreparationPolicy;
pub use prepared_formula::PreparedFormula;
pub use registry_identity::RegistryIdentity;
pub use source_fingerprint::SourceFingerprint;
