/// Invalid registration, protected builtin/ATP name, or conflicting registry update.
/// A failed registration leaves the existing registry unchanged.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExtensionRegistrationError {
    /// Function spelling is outside the accepted identifier grammar.
    #[error("invalid name")]
    InvalidName,
    /// Minimum argument count exceeds the maximum.
    #[error("invalid arity")]
    InvalidArity,
    /// No implementation revision signature was supplied.
    #[error("missing signature")]
    MissingSignature,
    /// Registration would override an implemented builtin.
    #[error("builtin")]
    Builtin,
    /// ATP registration names no supported placeholder.
    #[error("not atp placeholder")]
    NotAtpPlaceholder,
    /// User-defined registration uses a reserved ATP spelling.
    #[error("reserved atp name")]
    ReservedAtpName,
    /// A registration with this name already exists.
    #[error("already registered")]
    AlreadyRegistered,
    /// Replacement or removal targets an absent registration.
    #[error("not registered")]
    NotRegistered,
    /// Registry identity counters cannot advance further.
    #[error("epoch exhausted")]
    EpochExhausted,
}
