/// Run-local identifier linking an observed operation to its parent and completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId(
    /// Monotonically assigned identity within one run.
    pub u64,
);
