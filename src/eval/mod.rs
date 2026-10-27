//! Compile and evaluate spreadsheet models with direct or incremental calculation.
//!
//! Use [`run_cell`](crate::eval::run_cell) or [`run_formula`](crate::eval::run_formula) for a direct calculation against workbook data.
//! Use [`Evaluation`](crate::eval::overlay::Evaluation) for editable scenarios and
//! [`Overlay`](crate::eval::overlay::Overlay) to retain dependency
//! information and reuse results across input changes.
//!
//! Use [`Evaluation`](crate::eval::overlay::Evaluation) when you need calculated outputs without changing the original
//! workbook. For example, try several input values and compare the results:
//!
//! ```rust
//! use poirot::eval::{overlay::Evaluation, EvaluationContext};
//! use poirot::model::{CellValue, Workbook};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut book = Workbook::new();
//!     book.add_sheet("Inputs")?.set_formula("B1", "A1*2")?;
//!     let mut evaluation = Evaluation::with_workbook(&book)
//!         .build(EvaluationContext::default())?;
//!     let input = evaluation.target("Inputs!A1")?;
//!     let output = evaluation.target("Inputs!B1")?;
//!     for (input_value, expected) in [(21., 42.), (24., 48.)] {
//!         evaluation.set_value(input, input_value)?;
//!         assert_eq!(evaluation.cell(output)?, CellValue::Number(expected));
//!     }
//!     Ok(())
//! }
//! ```
//!
//! For ordinary execution without detailed counters, use the same builder's
//! `build_calculation(context)`. Its report uses `None` for unavailable profiling.
//! Existing `build(context)` keeps its detailed usage contract. Quotas are optional
//! through [`EvaluationLimits`](crate::eval::EvaluationLimits) and remain independent of profiling.
//!
//! Creating an evaluation does not calculate anything. `cell` calculates one output
//! and its dependencies; `outputs` accepts a list, and `all` calculates every occupied
//! cell and any formula spill cells. Each input edit clears calculated results.
//!
//! For repeated scenarios, [`Overlay`](crate::eval::overlay::Overlay) tracks dependencies and keeps unaffected results
//! between calculations. Its `evaluation` method starts a calculation with a supplied
//! [`EvaluationContext`](crate::eval::EvaluationContext). The context controls time, randomness, locale, and limits.
//!
//! Edits stay in the evaluation or overlay until explicitly applied. An overlay also
//! retains intermediate results; changed inputs, context settings, or volatile
//! functions invalidate affected results. Native snapshots can save state for later
//! restoration.
//!
//! Register custom functions through [`extensions`](crate::eval::functions::extensions) and attach the registry with the
//! builder's `with_extensions` method. Registrations can add application-specific
//! functions but cannot override existing builtins.
//!
//! Use the compiler, function registry, and custom-function interfaces when embedding
//! the evaluator in your own engine. For ordinary workbook editing and file handling,
//! start with [poirot](https://docs.rs/poirot).

/// Typed builtin adapters over the shared evaluation host.
pub mod adapters;
/// Bounded native workbook source and calculation checkpoint persistence.
pub mod artifact;
/// Formula compilation from text through bound expressions to bytecode programs.
pub mod compile;
/// Immutable evaluation inputs: policies, clock, random streams, and context fields.
mod config;
/// Cell identity and run-local hash tables and memo keyed by workbook cell coordinates.
mod coordinates;
/// Public direct entry points that evaluate cells, formulas and workbooks.
mod entry;
/// Function catalog: builtin registry, extension registrations, and external names.
pub mod functions;
/// Bridge from a session to builtin adapters through `CallHost`.
mod host;
/// Sparse scenario inputs and the effective workbook reads that merge them with a baseline.
mod inputs;
/// Operational limits: budgets, usage records, run counters, memo retention, failures and outcomes, and fallible allocation.
mod limits;
/// Conversion and guarded publication of evaluated values into workbook caches.
pub mod materialization;
/// Execution observation, bounded event retention and consumed builtin calls.
pub mod observe;
/// Sparse workbook edits, compiled formula reuse, and incremental evaluations.
pub mod overlay;
/// Caller-implemented run ports: completed cells, consumed reads and cached results.
mod ports;
/// Retained dependency evidence and result caches reused across runs.
mod reuse;
/// Evaluation runs over a prepared workbook view, their observers and reports.
pub mod run;
/// Per-run evaluator state and the executors that run one cell lifecycle over it.
mod session;
/// Dynamic-array discovery, settlement and publication.
mod spill;
/// Scalar and reference value semantics shared by the VM, host, and adapters.
mod values;
/// Bytecode execution, expression dispatch, operators, selection and broadcasting.
mod vm;

// Direct entry points and their per-cell results.
pub use entry::{
    ExtensionEvaluation, ExtensionExecution, run_cell, run_cell_with_configuration,
    run_cell_with_extensions, run_formula, run_formula_with_configuration,
    run_formula_with_extensions, run_plan, run_plan_with_configuration, run_plan_with_extensions,
    run_recalculation, run_recalculation_with_configuration, run_recalculation_with_receipts,
    run_workbook, run_workbook_with_configuration,
};

// Evaluation runs, observers, and completion reports.
pub use run::{
    EvaluationProbe, EvaluationRun, InputPreparationError, RunReport, SharedRunError,
    SharedWorkbook, WorkbookView,
};

// Read, completion and cached-result ports of a run.
pub use ports::{
    CachedCellReader, CellCompletion, CellObserver, CellProvenance, ContextRead, DependencyRead,
    DependencyReadSink, ReadEvent, same_result_identity,
};

// Sparse scenario inputs and effective workbook reads.
pub use inputs::{EffectiveStoredCells, EffectiveWorkbook, InputError, RunInputs};

// Run configuration, policy selections, and cache identity types.
pub use config::{
    ArrayResultPolicy, ComputationFingerprint, ConfigurationFingerprint, ContextFingerprint,
    CurrencyContext, DateOrder, EvaluationContext, EvaluationInputs, EvaluationPolicy,
    FormattingConfiguration, Localization, ProducerPolicy, RunConfiguration,
};

/// One calculated output returned by bulk evaluation.
pub use values::CellEvaluation;

/// Explicit numeric input tokens for locale-parametric argument parsing.
pub use poirot_parsing::number::NumberLocale;

/// Native regex guards, independently configurable from evaluator work quotas.
pub use poirot_functions::text::regex::ExecutionLimits as RegexSettings;

// Public evaluation limits, outcomes, failures, and usage counters.
pub use limits::{
    BudgetError, BudgetKind, CapabilityId, EvaluationBudgets, EvaluationError, EvaluationLimits,
    EvaluationOutcome, EvaluationReasonError, EvaluationUsage, MemoRetentionLimits,
    MemoRetentionUsage, RandomAccess, ResourceFailureKind, RunUsage, WorkKind,
};

// Cell identity and the coordinate-keyed memo of evaluated cells.
pub use coordinates::{CellMemo, EvaluationTarget};

// Formula compilation, prepared programs, and registry identities.
pub use compile::compiler::{
    CompiledFormula, CompiledWorkbook, CompilerUsage, FormulaCompiler, PreparationPolicy,
    PreparedFormula, RegistryIdentity, SourceFingerprint,
};

// Registered builtin metadata and advertised function names.
pub use functions::registry::{FUNCTION_DESCRIPTORS, SUPPORTED_FUNCTIONS};

// Observation of builtin handlers actually entered during execution.
pub use observe::{BuiltinInvocationObserver, ConsumedBuiltinInvocation};

// Lookup traversal axis, matching mode, and selection rules.
pub use adapters::search::execution::{SearchAxis, SearchMode, SearchSelection};

// Formula cache publication policies and receipts.
pub use materialization::{
    FormulaCachePublicationReceipt, FormulaCacheWriteReceipt, MaterializedScalarSummary,
};
