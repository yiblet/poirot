use super::computed_cell::ComputedCell;

/// A computed formula withheld from ordinary caches until ownership settles.
pub(super) struct Held {
    pub(super) result: ComputedCell,
    pub(super) source_cached: bool,
    pub(super) retained: bool,
}
