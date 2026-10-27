//! Pinned source identity of the saved-result setter.

/// Pinned POI revision and source digests defining saved-result setter behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellSetterSourceBinding {
    /// Pinned Apache POI revision.
    pub poi_revision: &'static str,
    /// Source digests defining the saved-result setter contract.
    pub source_sha256: &'static [(&'static str, &'static str)],
}
