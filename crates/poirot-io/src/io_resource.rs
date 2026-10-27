/// Caller-controlled import resource, measured independently of heap usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoResource {
    /// Compressed source bytes for one load.
    InputBytes,
    /// Expanded bytes in an individual XLSX entry.
    ZipEntryBytes,
    /// Expanded bytes across one XLSX package.
    ZipTotalBytes,
    /// Entries declared in one XLSX package.
    ZipEntries,
}
