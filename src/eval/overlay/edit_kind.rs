//! Content or style intent of an authored edit.

/// Whether an authored operation replaces content or preserves it while changing style.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    Content,
    Style,
}
