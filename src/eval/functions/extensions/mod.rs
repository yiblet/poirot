//! Add application-specific functions to spreadsheet formulas.
//!
//! Register a function, then attach its registry to an `Evaluation` or `Overlay`
//! builder with `with_extensions`, or use the direct entry points below. Providers
//! declare which inputs they can read and when their results can be reused.
//! Registration revisions identify the implementation used by compiled formulas.
//! ASCII names follow POI's ROOT case policy; Unicode casing needs a separate policy.
//!
//! Register a provider, then use [`crate::eval::run_formula_with_extensions`] or
//! [`crate::eval::run_cell_with_extensions`]. Names cannot override builtins. A deferred provider
//! can leave unused arguments unevaluated, as in this first-argument function:
//!
//! ```
//! use poirot::eval::{EvaluationOutcome, ExtensionExecution, run_formula_with_extensions};
//! use poirot::eval::{functions::extensions::*, functions::registry::Arity};
//! use poirot::model::{CellValue, Workbook};
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
//!
//! Depends on: config (evaluation inputs), coordinates (calling cell targets), limits
//! (budgets and failures) and values (spreadsheet failures); siblings registry (builtin
//! identities) and external_namespace (retained external names).

/// Binding-only resolver permits lazy compatibility trees without provider capture.
mod binding_resolver;
/// A specific registration selected while binding a formula.
mod extension_binding;
/// Evidence permitting reuse of an executed provider's result across evaluations.
mod extension_cacheability;
/// Workbook/context operations a provider is allowed to request through its host.
mod extension_capabilities;
/// A provider's accepted arity, evaluation order, capabilities, and reuse contract.
mod extension_contract;
/// When the engine resolves argument expressions before calling a provider.
mod extension_evaluation_strategy;
/// Provider failure separated into catchable spreadsheet errors and operational aborts.
mod extension_failure;
/// A registered formula implementation called through a budgeted [`ExtensionHost`].
mod extension_function;
/// Deferred, budgeted workbook access offered to extension providers.
mod extension_host;
/// Opaque executable registration ID assigned by a registry; not a stable serialized function name.
mod extension_id;
/// Exact registry snapshot used to bind and validate prepared formulas.
mod extension_identity;
/// One provider entry recorded with its exact binding.
mod extension_invocation;
/// Registration namespace: a user-defined name or an explicitly registerable Analysis ToolPak placeholder.
mod extension_namespace;
/// Argument kind and rectangular dimensions without traversing referenced cells.
mod extension_operand_description;
/// Argument channel before scalar coercion: omitted syntax, scalar, array, or reference.
mod extension_operand_kind;
/// Provider result: a scalar, rectangular row-major array, or unchanged argument channel.
mod extension_output;
/// Invalid registration, protected builtin/ATP name, or conflicting registry update.
mod extension_registration_error;
/// One registration's executable identity and declared behavior.
mod extension_registration_identity;
mod extension_registry;
/// Result of a provider or host operation with both failure kinds preserved.
mod extension_result;
/// One resolved argument value with omitted-argument identity preserved in `missing`.
mod extension_scalar;
/// Executed provider entries in invocation order.
mod extension_trace;

/// ATP placeholder names that callers may implement through explicit registration.
pub use crate::eval::functions::atp::REGISTERABLE_ATP_PLACEHOLDERS;
pub(in crate::eval) use binding_resolver::BindingResolver;
pub use extension_binding::ExtensionBinding;
pub use extension_cacheability::ExtensionCacheability;
pub use extension_capabilities::ExtensionCapabilities;
pub use extension_contract::ExtensionContract;
pub use extension_evaluation_strategy::ExtensionEvaluationStrategy;
pub use extension_failure::ExtensionFailure;
pub use extension_function::ExtensionFunction;
pub use extension_host::ExtensionHost;
pub use extension_id::ExtensionId;
pub use extension_identity::ExtensionIdentity;
pub use extension_invocation::ExtensionInvocation;
pub use extension_namespace::ExtensionNamespace;
pub use extension_operand_description::ExtensionOperandDescription;
pub use extension_operand_kind::ExtensionOperandKind;
pub use extension_output::ExtensionOutput;
pub use extension_registration_error::ExtensionRegistrationError;
pub use extension_registration_identity::ExtensionRegistrationIdentity;
/// Revisioned registration table owning executable providers.
pub use extension_registry::ExtensionRegistry;
pub use extension_result::ExtensionResult;
pub use extension_scalar::ExtensionScalar;
pub use extension_trace::ExtensionTrace;
