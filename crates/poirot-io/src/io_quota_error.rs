use crate::IoResource;

/// Actual claim rejected before import under an explicit caller quota.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "import {resource:?} quota {limit} exceeded (consumed {consumed}, requested {requested}) during {operation:?}"
)]
pub struct IoQuotaError {
    /// Rejected resource and unit.
    pub resource: IoResource,
    /// Configured maximum.
    pub limit: u64,
    /// Previously accepted amount when the resource is cumulative.
    pub consumed: u64,
    /// Additional amount requested by this operation.
    pub requested: u64,
    /// File, package entry, or import operation when known.
    pub operation: crate::IoOperation,
}
