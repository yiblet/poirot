/// Per-operation resource capacity checked separately from accumulated work.
#[derive(Clone, Copy)]
pub(super) enum Reservation {
    /// Logical collection element count.
    Values(u64),
    /// Text buffer storage bytes.
    TextBytes(usize),
    /// Wildcard state count for one match.
    WildcardStates(u64),
}
