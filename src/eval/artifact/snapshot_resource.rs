/// Resource admitted during native snapshot loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotResource {
    /// Entire source file bytes.
    FileBytes,
    /// Typed values across source and prepared sections.
    DecodedValues,
    /// Copied text and byte payload across sections.
    DecodedBytes,
    /// Caller-selected recursive nesting below the engine ceiling.
    Depth,
}
