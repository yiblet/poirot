use super::context_fingerprint::ContextFingerprint;
use super::formatting_configuration::FormattingConfiguration;

/// Context identity used to invalidate only results that consumed changed inputs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) struct ContextKey {
    /// Clock, seed, and localization identity.
    pub(in crate::eval) inputs: ContextFingerprint,
    /// Formatting identity, checked for cells that consumed formatting.
    pub(in crate::eval) formatting: FormattingConfiguration,
    /// Numeric input token identity for cells that parsed text.
    #[serde(default)]
    pub(in crate::eval) number_locale: poirot_parsing::number::NumberLocale,
}
