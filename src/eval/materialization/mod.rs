//! Explicit worksheet cache publication, independent of computed numeric results:
//! the saved-result policy and publication receipts.
//!
//! Depends on: coordinates (evaluation targets).

/// Pinned source identity of the saved-result setter.
mod cell_setter_source_binding;
/// Saved-result publication policy identity.
mod formula_cache_materialization_identity;
/// How computed scalars are stored as saved formula results.
mod formula_cache_materialization_policy;
/// Receipts of a completed cache publication.
mod formula_cache_publication_receipt;
/// Evidence of one prepared formula-cache write.
mod formula_cache_write_receipt;
/// Scalar summaries recorded in publication receipts.
mod materialized_scalar_summary;

pub use cell_setter_source_binding::CellSetterSourceBinding;
pub use formula_cache_materialization_identity::FormulaCacheMaterializationIdentity;
pub use formula_cache_materialization_policy::FormulaCacheMaterializationPolicy;
pub use formula_cache_publication_receipt::FormulaCachePublicationReceipt;
pub use formula_cache_write_receipt::FormulaCacheWriteReceipt;
pub use materialized_scalar_summary::MaterializedScalarSummary;
