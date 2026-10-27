/// Legacy lightweight metadata identity. Executable providers use
/// `extensions::ExtensionBinding` and the full explicit registry contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UdfIdentity {
    /// Caller-assigned function registration identity.
    pub registration: u64,
    /// Caller-assigned implementation version.
    pub version: u64,
}
