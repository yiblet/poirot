use super::snapshot_resource::SnapshotResource;

/// A configured native-snapshot quota refusal with actual decoder admission counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotQuotaFailure {
    /// File bytes, decoded values, decoded bytes, or caller-selected depth.
    pub resource: SnapshotResource,
    /// Configured maximum.
    pub limit: usize,
    /// Amount admitted before this refused operation.
    pub consumed: usize,
    /// Additional amount requested.
    pub requested: usize,
}
