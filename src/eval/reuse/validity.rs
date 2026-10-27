use super::key::Key;
use crate::eval::coordinates::CellMap;

/// Immediate invalidation evidence, separate from the last retained value.
#[derive(Clone, Default)]
pub(in crate::eval) enum Validity {
    /// The retained result is available to ordinary cache readers.
    #[default]
    Clean,
    /// A source/context change requires normal execution.
    Forced,
    /// One dirty input can settle without speculating over an unordered set of reads.
    Cause { key: Key, version: u64 },
    /// Multiple dirty inputs are settled in the retained value-read order.
    Causes(CellMap<u64>),
}

impl Validity {
    /// Storage used by additional dirty causes, apart from the inline validity tag.
    pub(super) fn retained_bytes(&self) -> usize {
        match self {
            Self::Causes(causes) => causes.len().saturating_mul(64),
            _ => 0,
        }
    }

    /// Whether the retained result can be published without further validation.
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }
}
