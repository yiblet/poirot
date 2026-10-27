//! Indexed criteria aggregation following pinned POI Countif, Sumif and Baseifs.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! functions (identities and failures), limits (budgets) and values (value channels and
//! criteria semantics); sibling elementwise (shared broadcast kernels).

/// Criteria family dispatch and its shared helpers.
mod dispatch;

/// Conversion between engine scalars and the criteria helper's value domain.
mod scalar_domain;

pub(in crate::eval) use dispatch::execute;
