//! XLS admission and decoding failures.
use std::fmt;

/// Malformed or unsupported XLS structure, an encrypted XLSX container, or a pre-decode resource-budget refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightError {
    /// Source claims exceed an explicit pre-decode resource limit.
    Resource {
        /// Resource category whose admission failed.
        resource: &'static str,
        /// Source amount requested before allocation.
        requested: u64,
        /// Configured maximum for this resource.
        limit: u64,
    },
    /// Compound-file or BIFF framing is structurally invalid.
    Malformed(&'static str),
    /// Source cannot be faithfully represented by the supported backend path.
    Representation(&'static str),
    /// The compound file is an encrypted (password-protected) OOXML package holding
    /// MS-OFFCRYPTO `EncryptionInfo` and `EncryptedPackage` streams, not an XLS workbook.
    EncryptedXlsx,
}

impl fmt::Display for PreflightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Resource {
                resource,
                requested,
                limit,
            } => write!(f, "{resource} requests {requested}, budget {limit}"),
            Self::Malformed(s) => write!(f, "malformed CFB/BIFF: {s}"),
            Self::Representation(s) => write!(f, "BIFF backend representation: {s}"),
            Self::EncryptedXlsx => {
                f.write_str("encrypted (password-protected) XLSX workbooks are not supported")
            }
        }
    }
}

impl std::error::Error for PreflightError {}
