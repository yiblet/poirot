/// Candidate comparison and duplicate-selection contract of a classic lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum ClassicLookupKind {
    /// MATCH wildcard and approximate-position semantics.
    Match,

    /// VLOOKUP/HLOOKUP first-axis table selection semantics.
    Table,
}
