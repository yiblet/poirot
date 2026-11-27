use crate::eval::limits::{EvalResult, EvaluationError};

/// Identifies the live lexical slots belonging to one formula invocation.
#[derive(Clone, Copy)]
pub(in crate::eval) struct LocalFrame {
    base: usize,
}

impl LocalFrame {
    /// Start a new frame after the caller's live lexical values.
    pub(super) fn new(base: usize) -> Self {
        Self { base }
    }

    /// Address one slot relative to this frame within the reusable stack.
    pub(super) fn index(self, slot: u32) -> EvalResult<usize> {
        self.base
            .checked_add(slot as usize)
            .ok_or(EvaluationError::StalePlan)
    }
}
