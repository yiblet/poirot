//! Receipts describing atomic worksheet formula-cache publication.
//!
//! Depends on: coordinates (evaluation targets).

/// Receipts of a completed cache publication.
mod formula_cache_publication_receipt;
/// Evidence of one prepared formula-cache write.
mod formula_cache_write_receipt;
/// Scalar summaries recorded in publication receipts.
mod materialized_scalar_summary;

pub use formula_cache_publication_receipt::FormulaCachePublicationReceipt;
pub use formula_cache_write_receipt::FormulaCacheWriteReceipt;
pub use materialized_scalar_summary::MaterializedScalarSummary;
