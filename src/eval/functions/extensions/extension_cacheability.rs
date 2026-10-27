/// Evidence permitting reuse of an executed provider's result across evaluations.
/// Thread safety (`Send + Sync`) does not establish any of these guarantees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExtensionCacheability {
    /// Tracked host reads, arguments, and implementation identity determine the result.
    /// Captured configuration must be covered by the implementation signature.
    Deterministic,
    /// Deterministic for tracked inputs and an immutable external-data snapshot.
    /// Replace the registration when that snapshot changes; the revision participates
    /// in binding, compilation, and serialized-cache compatibility checks.
    Versioned {
        /// Caller-managed revision identifying all external data used by this provider.
        external_inputs_revision: u64,
    },
    /// Refresh once reached in each new evaluation, even with unchanged tracked inputs.
    Volatile,
    /// Dependencies or external effects cannot be described reliably. Conservatively
    /// refresh each evaluation; calls can still be memoized within one evaluation.
    /// Side-effect counts and ambient state are outside deterministic result parity.
    Opaque,
}
