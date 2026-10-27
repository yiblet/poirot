use super::array_result_policy::ArrayResultPolicy;
use super::builtin_semantics_policy::BuiltinSemanticsPolicy;
use super::computation_fingerprint::ComputationFingerprint;
use super::context_fingerprint::ContextFingerprint;
use super::formatting_configuration::FormattingConfiguration;
use super::producer_policy::ProducerPolicy;
use poirot_functions::text_units::TextSemanticsPolicy;

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
    /// Selected text source semantics.
    pub text_policy: TextSemanticsPolicy,
    /// Selected numeric result semantics.

    /// Selected builtin compatibility behavior.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Selected array publication behavior.
    pub array_result_policy: ArrayResultPolicy,
    /// Producer computation or caller-trusted saved-source behavior.
    pub producer_policy: ProducerPolicy,
    /// Pinned numeric source identity, when selected.
    pub numeric_source: crate::eval::values::numeric_outcome::NumericSourceBinding,
    /// Saved-cache conversion identity.
    pub materialization: crate::eval::materialization::FormulaCacheMaterializationIdentity,
}

impl ConfigurationFingerprint {
    /// Extract policies that affect computation independently of consumed context fields.
    pub fn computation(&self) -> ComputationFingerprint {
        ComputationFingerprint {
            builtin_policy: self.builtin_policy,
            array_result_policy: self.array_result_policy,
            producer_policy: self.producer_policy,
            numeric_source: self.numeric_source,
        }
    }
}
