/// Formula-domain failures and checked allocation/geometry refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ModernTextError {
    /// Invalid argument domain.
    #[error("invalid argument domain")]
    Value,
    /// No selected delimiter occurrence.
    #[error("no selected delimiter occurrence")]
    NotFound,
    /// All fields were excluded.
    #[error("all fields were excluded")]
    Empty,
    /// Checked text or collection geometry exceeds representable storage.
    #[error("checked text or collection geometry exceeds representable storage")]
    SizeOverflow,
    /// A fallible reservation was refused by the allocator.
    #[error("a fallible reservation was refused by the allocator")]
    AllocationFailure,
    /// Supplied expected geometry does not match the independently scanned fields.
    #[error("supplied expected geometry does not match the independently scanned fields")]
    InvalidGeometry,
}
