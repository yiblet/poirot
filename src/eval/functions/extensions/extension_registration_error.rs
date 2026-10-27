/// Invalid registration, protected builtin/ATP name, or conflicting registry update.
/// A failed registration leaves the existing registry unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionRegistrationError {
    /// Function spelling is outside the accepted identifier grammar.
    InvalidName,
    /// Minimum argument count exceeds the maximum.
    InvalidArity,
    /// No implementation revision signature was supplied.
    MissingSignature,
    /// Registration would override an implemented builtin.
    Builtin,
    /// ATP registration names no supported placeholder.
    NotAtpPlaceholder,
    /// User-defined registration uses a reserved ATP spelling.
    ReservedAtpName,
    /// A registration with this name already exists.
    AlreadyRegistered,
    /// Replacement or removal targets an absent registration.
    NotRegistered,
    /// Registry identity counters cannot advance further.
    EpochExhausted,
}

impl std::fmt::Display for ExtensionRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "extension registration: {self:?}")
    }
}

impl std::error::Error for ExtensionRegistrationError {}
