//! XLS admission and decoding failures.

/// Malformed or unsupported XLS structure, an encrypted XLSX container, or a pre-decode resource-budget refusal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PreflightError {
    /// Source claims exceed an explicit pre-decode resource limit.
    #[error("{resource:?} requests {requested}, budget {limit}")]
    Resource {
        /// Resource category whose admission failed.
        resource: crate::BiffResource,
        /// Source amount requested before allocation.
        requested: u64,
        /// Configured maximum for this resource.
        limit: u64,
    },
    /// Compound-file or BIFF framing is structurally invalid.
    #[error("malformed CFB/BIFF structure")]
    Malformed(#[source] crate::BiffStructureError),
    /// Source cannot be faithfully represented by the supported backend path.
    #[error("BIFF backend representation refused input")]
    Representation(#[source] crate::BiffRepresentationError),
    /// The compound file is an encrypted (password-protected) OOXML package holding
    /// MS-OFFCRYPTO `EncryptionInfo` and `EncryptedPackage` streams, not an XLS workbook.
    #[error("encrypted (password-protected) XLSX workbooks are not supported")]
    EncryptedXlsx,
}
