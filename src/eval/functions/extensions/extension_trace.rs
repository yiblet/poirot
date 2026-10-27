use super::extension_invocation::ExtensionInvocation;

/// Executed extension calls in invocation order. Entries show entry, not successful completion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionTrace {
    /// Actual provider entries in execution order.
    pub invocations: Vec<ExtensionInvocation>,
}
