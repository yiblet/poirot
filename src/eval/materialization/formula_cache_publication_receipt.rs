//! Receipts of a completed cache publication.

use super::formula_cache_materialization_identity::FormulaCacheMaterializationIdentity;
use super::formula_cache_write_receipt::FormulaCacheWriteReceipt;

/// Per-formula write receipts from a successfully completed cache publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaCachePublicationReceipt {
    /// Policy shared by every write in this publication.
    pub identity: FormulaCacheMaterializationIdentity,
    /// Per-formula computed and stored result summaries.
    pub writes: Vec<FormulaCacheWriteReceipt>,
}
