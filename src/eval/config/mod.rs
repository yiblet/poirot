//! Immutable evaluation inputs: execution policies, clock, random streams, calendar
//! bounds, localization, and the context fields a cell may consume.
//!
//! Depends on: limits (work and retention limits of an evaluation context).

/// Worksheet publication of final array results.
mod array_result_policy;

/// Calculation calendar bounds and workbook serial conversion.
pub(in crate::eval) mod calendar;
/// Numeric conversion of captured evaluation instants.
pub(in crate::eval) mod clock;
/// Execution policies that affect raw computed values.
mod computation_fingerprint;
/// Exact configuration identity for comparison and diagnostics.
mod configuration_fingerprint;
/// Context categories reported when a consumed setting changes.
mod context_field;
/// Bit-exact identity of captured evaluation inputs.
mod context_fingerprint;
/// Context identity used to invalidate only results that consumed changed inputs.
mod context_key;
/// Context fields a cell actually consumed.
mod context_mask;
/// Caller-supplied currency formatting settings.
mod currency_context;
/// Numeric slash-date component ordering.
mod date_order;
/// Complete per-evaluation clock, formatting, work and retention settings.
mod evaluation_context;
/// Clock, random seed, and locale settings for a calculation.
mod evaluation_inputs;
/// Parsing and computation rules owned by an overlay or direct evaluation.
mod evaluation_policy;
/// Value-formatting inputs for formula execution.
mod formatting_configuration;
/// Fixed-offset localization policy.
mod localization;
/// Computed producers or caller-trusted saved results.
mod producer_policy;
/// Reproducible cell-local random streams.
pub(in crate::eval) mod random;
/// Immutable choices for one calculation.
mod run_configuration;
/// Unit tests for clock conversion and timestamp serialization.
#[cfg(test)]
mod tests;
/// Lossless Serde boundary for evaluation timestamps.
pub(in crate::eval) mod timestamp;

pub use array_result_policy::ArrayResultPolicy;
pub use computation_fingerprint::ComputationFingerprint;
pub use configuration_fingerprint::ConfigurationFingerprint;
pub use context_fingerprint::ContextFingerprint;
pub(in crate::eval) use context_key::ContextKey;
pub(in crate::eval) use context_mask::ContextMask;
pub use currency_context::CurrencyContext;
pub use date_order::DateOrder;
pub use evaluation_context::EvaluationContext;
pub use evaluation_inputs::EvaluationInputs;
pub use evaluation_policy::EvaluationPolicy;
pub use formatting_configuration::FormattingConfiguration;
pub use localization::Localization;
pub use producer_policy::ProducerPolicy;
pub use run_configuration::RunConfiguration;
