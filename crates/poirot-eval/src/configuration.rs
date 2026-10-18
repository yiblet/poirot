//! Explicit, immutable execution policy. Compilation never captures this policy.
//!
//! ```
//! use poirot_eval::{CurrencyContext, EvaluationInputs, FormattingConfiguration, RunConfiguration};
//! let currency = CurrencyContext {
//!     decimal_separator: ".".into(), grouping_separator: ",".into(),
//!     primary_group: 3, secondary_group: 3,
//!     positive_prefix: "$".into(), positive_suffix: "".into(),
//!     negative_prefix: "($".into(), negative_suffix: ")".into(),
//! };
//! let configuration = RunConfiguration::new(
//!     EvaluationInputs::default(), FormattingConfiguration::with_currency(currency));
//! assert!(configuration.formatting().currency().is_some());
//! ```
//!
//! The currency above is deliberately configured by the caller. Default policy
//! leaves currency unspecified; host locale is never read. Pass this policy to
//! `run_formula_with_configuration` or the other configured run entrypoints.
/// Explicit currency symbols, separators, grouping, and affixes.
pub use crate::adapters::financial::policy::CurrencyContext;
use crate::{ContextFingerprint, EvaluationInputs};
use poirot_text::{SourceVersion, TextSemanticsPolicy};

/// Caller-supplied currency formatting settings for formula execution.
/// Default leaves currency unspecified; the evaluator does not read the host locale.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FormattingConfiguration {
    currency: Option<CurrencyContext>,
}
impl FormattingConfiguration {
    /// Install caller-supplied currency formatting settings.
    pub fn with_currency(currency: CurrencyContext) -> Self {
        Self {
            currency: Some(currency),
        }
    }
    /// Borrow currency settings, or `None` when the caller did not supply them.
    pub fn currency(&self) -> Option<&CurrencyContext> {
        self.currency.as_ref()
    }
}
/// Builtin contracts selected at execution time. Compiled syntax and extension
/// bindings remain policy independent; the policy guards cached computation.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum BuiltinSemanticsPolicy {
    /// Preserve the existing registry and pinned source contracts.
    #[default]
    PinnedPoi,
    /// Documented Excel contracts. This is not an Excel observation/parity claim.
    ExcelDocumentation,
}

/// Worksheet publication of final array results, independent of builtin semantics.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ArrayResultPolicy {
    /// Reduce final arrays to a scalar using intersection behavior.
    #[default]
    ScalarIntersection,
    /// Publish rectangular results with tracked spill ownership and blocker checks.
    DynamicSpill,
}

/// Immutable choices for one calculation: clock/seed, localization, formatting,
/// text semantics, builtin behavior, numeric results, and array/cache publication.
/// Changing this configuration does not require reparsing formulas; cache reuse
/// checks the execution policies and fields relevant to each result.
#[derive(Clone, Debug)]
pub struct RunConfiguration {
    context: EvaluationInputs,
    formatting: FormattingConfiguration,
    number_locale: poirot_parsing::number::NumberLocaleV2,
    text_policy: TextSemanticsPolicy,
    numeric_policy: crate::NumericOutcomePolicy,
    builtin_policy: BuiltinSemanticsPolicy,
    array_result_policy: ArrayResultPolicy,
    materialization_policy: crate::FormulaCacheMaterializationPolicy,
}
impl RunConfiguration {
    /// Combine execution inputs and formatting with default computation policies.
    pub fn new(context: EvaluationInputs, formatting: FormattingConfiguration) -> Self {
        Self {
            context,
            formatting,
            number_locale: Default::default(),
            numeric_policy: crate::NumericOutcomePolicy::LegacyFinite,
            builtin_policy: BuiltinSemanticsPolicy::default(),
            array_result_policy: ArrayResultPolicy::default(),
            materialization_policy: crate::FormulaCacheMaterializationPolicy::PreserveComputed,
            text_policy: TextSemanticsPolicy {
                source: SourceVersion::PoirotF6c572d,
            },
        }
    }
    /// Return captured clock, seed, and localization inputs.
    pub fn context(&self) -> EvaluationInputs {
        self.context
    }
    /// Borrow configured value-formatting inputs.
    pub fn formatting(&self) -> &FormattingConfiguration {
        &self.formatting
    }
    /// Select explicit numeric input tokens, independently of display formatting.
    ///
    /// ```
    /// use poirot_eval::{NumberLocaleV2, RunConfiguration};
    /// let comma = NumberLocaleV2::new(",", Some("."), Some("EUR"), "%", ":")
    ///     .ok_or("invalid numeric locale")?;
    /// let configuration = RunConfiguration::default().with_number_locale(comma);
    /// # Ok::<(), &str>(())
    /// ```
    pub fn with_number_locale(mut self, locale: poirot_parsing::number::NumberLocaleV2) -> Self {
        self.number_locale = locale;
        self
    }
    /// Numeric text input profile. No ambient locale is consulted.
    pub fn number_locale(&self) -> &poirot_parsing::number::NumberLocaleV2 {
        &self.number_locale
    }
    /// Select a source-version policy explicitly. Existing constructors preserve legacy ROI units.
    pub fn with_text_policy(mut self, text_policy: TextSemanticsPolicy) -> Self {
        self.text_policy = text_policy;
        self
    }
    /// Return the selected text source semantics.
    pub fn text_policy(&self) -> TextSemanticsPolicy {
        self.text_policy
    }
    /// Select numeric result classification behavior.
    pub fn with_numeric_policy(mut self, policy: crate::NumericOutcomePolicy) -> Self {
        self.numeric_policy = policy;
        self
    }
    /// Select builtin compatibility behavior.
    pub fn with_builtin_policy(mut self, policy: BuiltinSemanticsPolicy) -> Self {
        self.builtin_policy = policy;
        self
    }
    /// Return the selected builtin compatibility behavior.
    pub fn builtin_policy(&self) -> BuiltinSemanticsPolicy {
        self.builtin_policy
    }
    /// Select scalar intersection or dynamic spill publication.
    pub fn with_array_result_policy(mut self, policy: ArrayResultPolicy) -> Self {
        self.array_result_policy = policy;
        self
    }
    /// Return the selected array publication behavior.
    pub fn array_result_policy(&self) -> ArrayResultPolicy {
        self.array_result_policy
    }
    /// Return the selected numeric result behavior.
    pub fn numeric_policy(&self) -> crate::NumericOutcomePolicy {
        self.numeric_policy
    }
    /// Select conversion behavior for saved workbook formula caches.
    pub fn with_materialization_policy(
        mut self,
        policy: crate::FormulaCacheMaterializationPolicy,
    ) -> Self {
        self.materialization_policy = policy;
        self
    }
    /// Return the saved-cache conversion policy.
    pub fn materialization_policy(&self) -> crate::FormulaCacheMaterializationPolicy {
        self.materialization_policy
    }
    /// Reject incompatible computation and cache materialization policies.
    pub fn validate(&self) -> Result<(), crate::EvaluationFailure> {
        self.materialization_policy.validate(self.numeric_policy)
    }
    /// Complete exact identity, including text source and every owned currency field.
    pub fn fingerprint(&self) -> ConfigurationFingerprint {
        ConfigurationFingerprint {
            context: self.context.fingerprint(),
            formatting: self.formatting.clone(),
            number_locale: self.number_locale.clone(),
            text_policy: self.text_policy,
            numeric_policy: self.numeric_policy,
            builtin_policy: self.builtin_policy,
            array_result_policy: self.array_result_policy,
            numeric_source: self.numeric_policy.source_binding(),
            materialization: self.materialization_policy.identity(),
        }
    }
}
impl Default for RunConfiguration {
    fn default() -> Self {
        Self::from(EvaluationInputs::default())
    }
}
impl From<EvaluationInputs> for RunConfiguration {
    fn from(context: EvaluationInputs) -> Self {
        Self::new(context, FormattingConfiguration::default())
    }
}
/// Exact configuration identity for comparison and diagnostics, including owned locale data.
/// Despite its name, equality compares the captured settings rather than only a hash.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ConfigurationFingerprint {
    /// Captured clock, seed, and localization identity.
    pub context: ContextFingerprint,
    /// Exact formatting settings.
    pub formatting: FormattingConfiguration,
    /// Exact numeric input token profile.
    pub number_locale: poirot_parsing::number::NumberLocaleV2,
    /// Selected text source semantics.
    pub text_policy: TextSemanticsPolicy,
    /// Selected numeric result semantics.
    pub numeric_policy: crate::NumericOutcomePolicy,
    /// Selected builtin compatibility behavior.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Selected array publication behavior.
    pub array_result_policy: ArrayResultPolicy,
    /// Pinned numeric source identity, when selected.
    pub numeric_source: Option<crate::NumericSourceBinding>,
    /// Saved-cache conversion identity.
    pub materialization: crate::FormulaCacheMaterializationIdentity,
}

/// Core computation projection. Consumed formatting/text/context remain separately
/// guarded by the existing cache masks; publication never changes raw computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputationFingerprint {
    /// Numeric result behavior affecting raw computation.
    pub numeric_policy: crate::NumericOutcomePolicy,
    /// Builtin compatibility behavior affecting raw computation.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Array publication behavior affecting computed results.
    pub array_result_policy: ArrayResultPolicy,
    /// Pinned numeric implementation identity, when selected.
    pub numeric_source: Option<crate::NumericSourceBinding>,
}
impl ConfigurationFingerprint {
    /// Extract policies that affect computation independently of consumed context fields.
    pub fn computation(&self) -> ComputationFingerprint {
        ComputationFingerprint {
            numeric_policy: self.numeric_policy,
            builtin_policy: self.builtin_policy,
            array_result_policy: self.array_result_policy,
            numeric_source: self.numeric_source,
        }
    }
}
