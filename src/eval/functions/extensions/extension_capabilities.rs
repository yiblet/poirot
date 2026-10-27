/// Workbook/context operations a provider is allowed to request through its host.
/// No default: a provider must explicitly declare its possible reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExtensionCapabilities {
    /// Permit reference arguments and reference cell reads.
    pub references: bool,
    /// Permit array arguments and rectangular array output.
    pub arrays: bool,
    /// Permit reading the evaluation position and captured clock/seed/localization.
    pub context: bool,
}
