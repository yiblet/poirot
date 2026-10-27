/// Semantic rejection of persisted artifact state.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotConstraintError {
    /// Recursive decoder nesting ceiling is 256.
    #[error("recursive decoder nesting ceiling is 256")]
    RecursiveDecoderNestingCeilingIs256,
}
