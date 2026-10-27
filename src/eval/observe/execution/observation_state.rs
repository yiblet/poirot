use super::operation::Operation;

/// Assigns observation identities and remembers the currently executing parent operation.
/// Nested expression and argument callbacks use this state to pair their entry
/// and completion events within one run.
#[derive(Default)]
pub(in crate::eval) struct ObservationState {
    /// Last assigned operation identity.
    pub(in crate::eval) next: u64,
    /// Active operation and the number of children it has entered.
    pub(in crate::eval) frame: Option<(Operation, u64)>,
}
