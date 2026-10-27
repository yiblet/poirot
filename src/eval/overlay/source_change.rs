//! Whether an authored mutation changed only literal values.

/// Whether value-only dependency cutoffs are sound for an authored mutation.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceChange {
    /// Literal content changed without changing formula or style metadata.
    Value,
    /// Formula, style, or stored spill ownership may have changed.
    Metadata,
}
