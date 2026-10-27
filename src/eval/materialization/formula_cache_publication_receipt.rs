//! Receipts of a completed cache publication.

use super::formula_cache_write_receipt::FormulaCacheWriteReceipt;

/// Per-formula write receipts from a successfully completed cache publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaCachePublicationReceipt {
    /// Formula targets and their saved scalar values.
    pub writes: Vec<FormulaCacheWriteReceipt>,
}
