//! Add application-specific functions to spreadsheet formulas.
//!
//! Register a function, then attach its registry to an `Evaluation` or `Overlay`
//! builder with `with_extensions`, or use the direct entry points below. Providers
//! declare which inputs they can read and when their results can be reused.
//! Registration revisions identify the implementation used by compiled formulas.
//! ASCII names follow POI's ROOT case policy; Unicode casing needs a separate policy.
//!
//! Register a provider, then use [`crate::extensions::run_formula_with_extensions`] or
//! [`crate::extensions::run_cell_with_extensions`]. Names cannot override builtins. A deferred provider
//! can leave unused arguments unevaluated, as in this first-argument function:
//!
//! ```
//! use poirot_eval::{EvaluationOutcome, extensions::*, registry::Arity};
//! use poirot_model::{CellValue, Workbook};
//! let mut registry = ExtensionRegistry::new();
//! registry.register(
//!     ExtensionNamespace::UserDefined, "USER_FIRST", "first/v1",
//!     ExtensionContract {
//!         arity: Arity { minimum: 2, maximum: 2 },
//!         evaluation: ExtensionEvaluationStrategy::Deferred,
//!         capabilities: ExtensionCapabilities { references: false, arrays: false, context: false },
//!         cacheability: ExtensionCacheability::Deterministic,
//!     },
//!     |host: &mut dyn ExtensionHost| {
//!         host.charge_work(1)?;
//!         Ok(ExtensionOutput::Argument(0))
//!     },
//! )?;
//! let mut workbook = Workbook::new();
//! workbook.add_sheet("Sheet1")?;
//! let result = run_formula_with_extensions(
//!     &workbook, 0, "A1".parse()?, "USER_FIRST(7,1/0)",
//!     ExtensionExecution { registry: &registry, context: Default::default(), budgets: Default::default() },
//! );
//! assert!(matches!(result.outcome, EvaluationOutcome::Complete {
//!     value: CellValue::Number(7.0), ..
//! }));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
/// ATP placeholder names that callers may implement through explicit registration.
pub use crate::atp::ATP_PLACEHOLDERS as REGISTERABLE_ATP_PLACEHOLDERS;
use crate::{
    CellError, CellValue, EvaluationInputs, EvaluationTarget, budget::EvaluationFailure,
    registry::Arity,
};

/// Registration namespace: a user-defined name or an explicitly registerable
/// Analysis ToolPak placeholder. Builtin implementations cannot be overridden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExtensionNamespace {
    /// Caller-defined function outside the protected builtin namespace.
    UserDefined,
    /// Explicitly registerable Analysis ToolPak placeholder.
    AnalysisToolPak,
}
/// Evidence permitting reuse of an executed provider's result across evaluations.
/// Thread safety (`Send + Sync`) does not establish any of these guarantees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExtensionCacheability {
    /// Tracked host reads, arguments, and implementation identity determine the result.
    /// Captured configuration must be covered by the implementation signature.
    Deterministic,
    /// Deterministic for tracked inputs and an immutable external-data snapshot.
    /// Replace the registration when that snapshot changes; the revision participates
    /// in binding, compilation, and serialized-cache compatibility checks.
    Versioned {
        /// Caller-managed revision identifying all external data used by this provider.
        external_inputs_revision: u64,
    },
    /// Refresh once reached in each new evaluation, even with unchanged tracked inputs.
    Volatile,
    /// Dependencies or external effects cannot be described reliably. Conservatively
    /// refresh each evaluation; calls can still be memoized within one evaluation.
    /// Side-effect counts and ambient state are outside deterministic result parity.
    Opaque,
}
/// Workbook/context operations a provider is allowed to request through its host.
/// No default: a provider must explicitly declare its possible reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExtensionCapabilities {
    /// Permit reference arguments and reference cell reads.
    pub references: bool,
    /// Permit array arguments and rectangular array output.
    pub arrays: bool,
    /// Permit reading the evaluation position and captured clock/seed/localization.
    pub context: bool,
}
/// When the engine resolves argument expressions before calling a provider.
/// Neither strategy automatically traverses all cells of a reference or array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExtensionEvaluationStrategy {
    /// Resolve argument expressions in order and check allowed kinds before the
    /// arity check and provider call. Reference contents remain lazy.
    Eager,
    /// Check arity first; resolve only arguments requested by the provider.
    /// Unrequested expressions, including errors or volatile calls, stay unevaluated.
    Deferred,
}
/// A provider's accepted arity, evaluation order, capabilities, and reuse contract.
/// Declare all workbook/context access the implementation may request; unsupported
/// access aborts operationally instead of silently inventing data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionContract {
    /// Accepted syntactic argument count.
    pub arity: Arity,
    /// Ordering of argument resolution and arity validation.
    pub evaluation: ExtensionEvaluationStrategy,
    /// Host access the provider is permitted to request.
    pub capabilities: ExtensionCapabilities,
    /// Explicit evidence permitting reuse, independent of provider thread safety.
    pub cacheability: ExtensionCacheability,
}
/// Exact registry snapshot used to bind and validate prepared formulas.
/// Includes registration revisions and external-name identity, not just function names.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionIdentity {
    /// Registry revision after successful namespace or registration updates.
    pub epoch: u64,
    /// Ordered external function namespace, when installed.
    pub external_namespace: Option<crate::external_namespace::ExternalFunctionNamespaceIdentity>,
    /// Exact registration identities in registry order.
    pub registrations: Vec<ExtensionRegistrationIdentity>,
}
/// One registration's executable identity and declared behavior.
/// Change `implementation_signature` whenever implementation or captured configuration changes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionRegistrationIdentity {
    /// Exact opaque executable binding identity, including insertion assignment.
    pub binding_id: ExtensionId,
    /// Canonical registered spelling.
    pub name: String,
    /// User-defined or ATP registration namespace.
    pub namespace: ExtensionNamespace,
    /// Caller supplied revision of the implementation AND captured configuration.
    pub implementation_signature: String,
    /// Declared execution behavior and host access.
    pub contract: ExtensionContract,
}
/// A specific registration selected while binding a formula. Registry replacement
/// creates a new binding; old plans must be rebound before using the replacement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionBinding {
    /// Opaque executable registration selected during binding.
    pub id: ExtensionId,
    /// Exact registration contract guarding execution.
    pub registration: ExtensionRegistrationIdentity,
}
/// Opaque executable registration ID assigned by a registry; not a stable serialized function name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExtensionId(u64);
/// Argument channel before scalar coercion: omitted syntax, scalar, array, or reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionOperandKind {
    /// Omitted syntactic argument.
    Missing,
    /// Single scalar value, including spreadsheet errors.
    Scalar,
    /// Materialized rectangular array.
    Array,
    /// Deferred workbook reference.
    Reference,
}
/// Argument kind and rectangular dimensions without traversing referenced cells.
/// Describing an argument can evaluate its expression to determine its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionOperandDescription {
    /// Operand channel before scalar coercion.
    pub kind: ExtensionOperandKind,
    /// Number of rows in the operand shape.
    pub rows: usize,
    /// Number of columns in the operand shape.
    pub columns: usize,
}
/// One resolved argument value with omitted-argument identity preserved in `missing`.
/// Spreadsheet errors remain in `value` for the provider to inspect.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionScalar {
    /// Resolved scalar, preserving spreadsheet errors as values.
    pub value: CellValue,
    /// Whether this value originated from an omitted argument.
    pub missing: bool,
}
/// Provider failure separated into catchable spreadsheet errors and operational aborts.
/// IFERROR may handle `Spreadsheet`; it cannot handle `Operational`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionFailure {
    /// Catchable semantic error returned by a provider or host coercion.
    Spreadsheet(CellError),
    /// Operational abort that formula error handlers cannot catch.
    Operational(EvaluationFailure),
}
impl From<CellError> for ExtensionFailure {
    fn from(value: CellError) -> Self {
        Self::Spreadsheet(value)
    }
}
impl From<EvaluationFailure> for ExtensionFailure {
    fn from(value: EvaluationFailure) -> Self {
        Self::Operational(value)
    }
}
/// Result of a provider/host operation, preserving spreadsheet and operational failures.
pub type ExtensionResult<T> = Result<T, ExtensionFailure>;
/// Reads are deferred and budgeted. Errors remain spreadsheet values for providers to inspect.
pub trait ExtensionHost {
    /// Number of syntactic arguments, including omitted positions.
    fn argument_count(&self) -> usize;
    /// Current formula cell. Requires the context capability.
    fn target(&mut self) -> ExtensionResult<EvaluationTarget>;
    /// Captured clock, random seed, and localization. Requires the context capability.
    fn context(&mut self) -> ExtensionResult<EvaluationInputs>;
    /// Resolve an argument expression and inspect its shape without reading its members.
    /// Argument indices are zero-based; required capabilities are checked before access.
    fn describe(&mut self, index: usize) -> ExtensionResult<ExtensionOperandDescription>;
    /// Select one scalar using the caller-cell intersection rules. Omitted arguments
    /// return a blank value with `missing = true`; cell errors remain inspectable data.
    fn scalar(&mut self, index: usize) -> ExtensionResult<ExtensionScalar>;
    /// Read a zero-based row/column offset within an argument array or reference.
    /// Only the requested cell is read; operational limits remain in force.
    fn read_cell(
        &mut self,
        index: usize,
        row: usize,
        column: usize,
    ) -> ExtensionResult<ExtensionScalar>;
    /// Charge provider-owned work before performing it. Exceeding a limit aborts the run.
    fn charge_work(&mut self, units: u64) -> ExtensionResult<()>;
}
/// Provider result: a scalar, rectangular row-major array, or unchanged argument channel.
/// Array outputs require the arrays capability; final scalar/array handling follows the run API.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionOutput {
    /// Scalar result, including semantic spreadsheet errors.
    Scalar(CellValue),
    /// Rectangular row-major result requiring the arrays capability.
    Array(Vec<Vec<CellValue>>),
    /// Preserve a requested argument's reference/array/missing channel.
    Argument(usize),
}
/// A registered formula implementation called through a budgeted [`ExtensionHost`].
/// Closures implementing the matching signature can be registered directly. Charge
/// provider-owned work through the host; do not use ambient clock/random/workbook state
/// that is absent from the declared cacheability contract and implementation revision.
pub trait ExtensionFunction: Send + Sync {
    /// Execute the provider through its declared, budgeted host capabilities.
    fn evaluate(&self, host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput>;
}
impl<F> ExtensionFunction for F
where
    F: Fn(&mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> + Send + Sync,
{
    fn evaluate(&self, host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
        self(host)
    }
}
/// Invalid registration, protected builtin/ATP name, or conflicting registry update.
/// A failed registration leaves the existing registry unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionRegistrationError {
    /// Function spelling is outside the accepted identifier grammar.
    InvalidName,
    /// Minimum argument count exceeds the maximum.
    InvalidArity,
    /// No implementation revision signature was supplied.
    MissingSignature,
    /// Registration would override an implemented builtin.
    Builtin,
    /// ATP registration names no supported placeholder.
    NotAtpPlaceholder,
    /// User-defined registration uses a reserved ATP spelling.
    ReservedAtpName,
    /// A registration with this name already exists.
    AlreadyRegistered,
    /// Replacement or removal targets an absent registration.
    NotRegistered,
    /// Registry identity counters cannot advance further.
    EpochExhausted,
}
impl std::fmt::Display for ExtensionRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "extension registration: {self:?}")
    }
}
impl std::error::Error for ExtensionRegistrationError {}

mod catalog;
mod execution;
mod invocation;

/// Revisioned registration table owning executable providers.
pub use catalog::ExtensionRegistry;
// Extension-aware execution entry points and invocation reports.
pub use execution::{
    ExtensionEvaluation, ExtensionExecution, ExtensionInvocation, ExtensionTrace,
    run_cell_with_extensions, run_formula_with_extensions, run_plan_with_extensions,
};
/// Shared argument admission and provider invocation path.
pub(crate) use invocation::execute;
