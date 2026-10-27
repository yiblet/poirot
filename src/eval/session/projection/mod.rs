//! Value projection that needs the run: final worksheet results, legacy scalar
//! selection and context-dependent numeric coercion.
//!
//! Depends on: the parent session state and its cells and reads executors, vm
//! (rectangular traversal), spill (staged spill results), ports (context reads), config
//! (date and builtin policies), coordinates, limits and values.

/// Context-dependent scalar coercion.
mod coercer;
/// Worksheet result and legacy scalar projection.
mod result_projection;

pub(in crate::eval) use coercer::Coercer;
pub(in crate::eval) use result_projection::ResultProjection;
