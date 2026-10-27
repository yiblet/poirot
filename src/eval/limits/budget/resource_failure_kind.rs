/// Operational cause distinct from a configured quota or unsupported spreadsheet feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceFailureKind {
    /// A checked size or work estimate cannot be represented on this platform.
    SizeOverflow,
    /// The allocator refused a fallible reservation.
    Allocation,
    /// A native backend stopped under its own execution or representation guards.
    BackendGuard,
}
