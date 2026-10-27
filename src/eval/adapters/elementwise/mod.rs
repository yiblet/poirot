//! Typed scalar, collection, and broadcast roles with independent argument demand.
//! Eager signatures admit geometry; selectors bind deferred roles without admission.
//!
//! Depends on: host (CallHost), vm (deferred arguments and broadcasting), spill (spill
//! extents), config (policies), limits (budgets and copying), values (value channels)
//! and observe (error-handler observation); siblings reference and text (shared
//! kernels).

/// A source shape whose binding never reads a member or coerces a value.
mod argument_role;
/// Argument role whose shape contributes to the output rectangle and whose value is read at each output coordinate.
mod broadcast;
pub(super) mod collections;
pub(super) mod controls;
/// Eager and lazy argument demand policies.
mod demand;
/// Argument role for one scalar control selected with the host's scalar-intersection rules.
mod fixed_scalar;
/// A context-aware scalar kernel with explicit typed argument roles.
mod kernel;
/// Carries broadcast shape planning into member execution.
mod lift;
pub(super) mod matrices;
/// Connects a kernel's current output coordinate to the shared broadcast plan and evaluator services.
mod member_context;
/// A role whose trailing source argument may be absent; explicit omissions and blanks remain present.
mod optional;
/// Owns a value after a deferred argument has been selected and evaluated.
mod prepared;
pub(super) mod recovery;
pub(super) mod reference;
pub(super) mod rows;
pub(super) mod selection;
mod signatures;
pub(super) mod source;
#[cfg(test)]
mod tests;
pub(super) mod text;
/// A tuple of typed roles with shared eager broadcast admission.
mod tuple_signature;
/// Argument role for an entire source collection, such as a statistical sample.
mod whole_collection;
/// Changes when an argument role may be inspected without changing its scalar, collection, or broadcast shape.
mod with_demand;

pub(super) use argument_role::ArgumentRole;
pub(super) use broadcast::Broadcast;
pub(super) use demand::Eager;
pub(super) use fixed_scalar::FixedScalar;
pub(super) use kernel::{Kernel, execute};
pub(super) use member_context::MemberContext;
pub(super) use optional::Optional;
pub(super) use prepared::Prepared;
pub(super) use signatures::{
    BroadcastCollections, NumericArguments, NumericErrorAdmission, NumericParameter,
    NumericSignature, PreparedSources, Variadic,
};
pub(super) use source::SourceIdentity;
pub(super) use tuple_signature::Signature;
pub(super) use whole_collection::WholeCollection;
pub(super) use with_demand::{Deferred, WithDemand};
