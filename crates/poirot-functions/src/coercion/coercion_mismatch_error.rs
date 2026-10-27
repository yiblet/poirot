use crate::coercion::ExpectedInput;

/// A candidate does not accept these inputs; only this failure permits fallback.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoercionMismatchError {
    /// Wrong number of arguments for a candidate signature.
    #[error("expected {expected} arguments; received {actual}")]
    Arity {
        /// Number of arguments required by this candidate.
        expected: usize,
        /// Number of supplied slots, including explicit omissions.
        actual: usize,
    },
    /// A selected scalar does not meet the requested rule.
    #[error("argument {argument:?} requires {expected:?}")]
    Input {
        /// Zero-based argument index, or `None` when no argument position is attached.
        argument: Option<usize>,
        /// Description of the value category accepted by the rule.
        expected: ExpectedInput,
    },
}
