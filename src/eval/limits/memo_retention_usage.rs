/// Logical memo entries and retained bytes, using the same units as [`MemoRetentionLimits`](super::MemoRetentionLimits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoRetentionUsage {
    /// Number of retained memo entries.
    pub cells: usize,
    /// Retained logical storage bytes.
    pub bytes: usize,
}
