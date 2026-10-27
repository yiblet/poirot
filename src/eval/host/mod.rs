//! Deferred, budgeted bridge from a running session to function adapters.
//!
//! `CallHost` is the adapter call boundary: it owns one call's deferred arguments, forces
//! them on demand into VM operand slots, admits and charges the run's budgets, and builds
//! attributed failures. Other behavior over a call runs through executor structs that
//! borrow the host, each named for its job: `OperandReader` (shapes, coordinates and
//! collections), `ArgumentCoercion` (numbers, text and equality), `ContextReads` (clock,
//! calendar, locale and currency),
//! `RandomDraws` (per-cell random streams), `SelectionAccess` (selected arguments,
//! rectangles and branch masks), `DynamicReferences` (sheet names, cells and reference
//! text), `LookupService`, `SubtotalReader` and `ErrorHandler`. `execute_extension` runs a registered provider
//! behind a capability-checked view of the host. This module does not resolve function
//! names or select a family implementation.
//!
//! Depends on: session (cell reads, coercion, recording and projection), vm (expression
//! execution, selection and broadcasting), spill (spill extents), compile (bound
//! arguments and aggregation plans), config (calendar and random streams), functions
//! (function identities and extension contracts), coordinates (cell targets), limits
//! (budgets and copying), values (value channels and reference geometry), adapters
//! (classic lookup execution), ports (dependency reads), inputs (effective workbook
//! cells) and observe (argument and error-handler observation).

/// Lazy aggregation-role projection and scoped alias resolution.
mod aggregators;
/// Numeric, text and equality coercion of a call's scalars.
mod argument_coercion;
/// Argument roles are fixed by the consumer before inspecting or projecting a value.
mod argument_demand;
/// Budgeted bridge from function adapters to a running evaluator and its deferred arguments.
mod call_host;
/// Selected worksheet read planned without reading cells.
mod cell_read_plan;
/// Candidate comparison contract of a classic lookup.
mod classic_lookup_kind;
/// Captured clock, calendar, locale and currency reads with their recorded dependencies.
mod context_reads;
/// Sheet names, worksheet cells and dynamic reference text for reference builders.
mod dynamic_references;
/// Error-handler observation and fallback evaluation.
mod error_handler;
/// Capability-checked provider invocation for registered extension functions.
mod extension_invocation;
/// Evaluated argument retained before scalar, array or reference access.
mod host_operand;
/// Classic lookup candidate selection and result reads.
mod lookup_service;
/// Explicit treatment of omitted syntax during scalar selection.
mod missing_policy;
/// Coercion of omitted scalar arguments under a missing-value policy.
mod missing_scalar;
/// Numeric coercion contract and its pure grammar decisions.
/// Operand shape and channel inspected without traversing member cells.
mod operand_description;
/// Value channel of an operand before scalar coercion.
mod operand_kind;
/// Shape, coordinate and collection reads of deferred operands.
mod operand_reader;
/// Builtin, text and regex policy reads with their recorded dependencies.
/// Budgeted draws from the calling cell's reproducible random stream.
mod random_draws;
/// Presents one deferred argument through the function-core scalar input interface.
mod scalar_input;
/// Selected scalar with its coercion provenance and omission flag.
mod scalar_operand;
/// Caller-cell scalar selection from arrays, without reading cells.
mod scalar_selection;
/// Reads selected array members while the evaluator builds a rectangular result.
mod selected_reader;
/// Selected arguments, selected rectangles and branch masks for selecting consumers.
mod selection_access;
/// Visibility-gated SUBTOTAL member reads.
mod subtotal_reader;
#[cfg(test)]
mod tests;
/// Bulk traversal policies for host contract tests.
#[cfg(test)]
mod traversal_policy;

pub(in crate::eval) use aggregators::Aggregators;
pub(in crate::eval) use argument_coercion::ArgumentCoercion;
pub(in crate::eval) use call_host::CallHost;
pub(in crate::eval) use classic_lookup_kind::ClassicLookupKind;
pub(in crate::eval) use context_reads::ContextReads;
pub(in crate::eval) use dynamic_references::DynamicReferences;
pub(in crate::eval) use error_handler::ErrorHandler;
pub(in crate::eval) use extension_invocation::execute_extension;
pub(in crate::eval) use host_operand::HostOperand;
pub(in crate::eval) use lookup_service::LookupService;
pub(in crate::eval) use missing_policy::MissingPolicy;
pub(in crate::eval) use operand_description::OperandDescription;
pub(in crate::eval) use operand_kind::OperandKind;
pub(in crate::eval) use operand_reader::OperandReader;
pub(in crate::eval) use random_draws::RandomDraws;
pub(in crate::eval) use scalar_operand::ScalarOperand;
pub(in crate::eval) use selected_reader::SelectedReader;
pub(in crate::eval) use selection_access::SelectionAccess;
pub(in crate::eval) use subtotal_reader::SubtotalReader;
