/// Whether possible dynamic producers execute or consume caller-trusted saved results.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ProducerPolicy {
    /// Compute demanded spill producers and their necessary competitors.
    #[default]
    Compute,
    /// Under DynamicSpill, consume saved source results without executing possible producers.
    /// Ordinary scalar formulas still execute. Missing scalar caches are indistinguishable
    /// from explicit Blank; both remain Blank. Complete declared arrays restore atomically;
    /// incomplete, undeclared owned, or child-edited arrays fail without recalculation.
    /// Replacing the anchor's content discards its baseline saved footprint.
    /// Saved values can deliberately remain stale across source and context changes.
    TrustSaved,
}
