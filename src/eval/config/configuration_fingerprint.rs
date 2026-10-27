use super::array_result_policy::ArrayResultPolicy;
use super::computation_fingerprint::ComputationFingerprint;
use super::context_fingerprint::ContextFingerprint;
use super::formatting_configuration::FormattingConfiguration;
use super::producer_policy::ProducerPolicy;

/// Exact configuration identity for comparison and diagnostics, including owned locale data.
/// Despite its name, equality compares the captured settings rather than only a hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigurationFingerprint {
    /// Captured clock, seed, and localization identity.
    pub context: ContextFingerprint,
    /// Exact formatting settings.
    pub formatting: FormattingConfiguration,
    /// Exact numeric input token profile.
    pub number_locale: poirot_parsing::number::NumberLocale,
    /// Selected Excel language class for legacy byte text functions.
    pub byte_text_language: super::byte_text_language::ByteTextLanguage,

    /// Selected array publication behavior.
    pub array_result_policy: ArrayResultPolicy,
    /// Producer computation or caller-trusted saved-source behavior.
    pub producer_policy: ProducerPolicy,
}

impl ConfigurationFingerprint {
    /// Extract policies that affect computation independently of consumed context fields.
    pub fn computation(&self) -> ComputationFingerprint {
        ComputationFingerprint {
            array_result_policy: self.array_result_policy,
            producer_policy: self.producer_policy,
        }
    }
}
