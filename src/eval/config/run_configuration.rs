use super::array_result_policy::ArrayResultPolicy;
use super::builtin_semantics_policy::BuiltinSemanticsPolicy;
use super::configuration_fingerprint::ConfigurationFingerprint;
use super::evaluation_inputs::EvaluationInputs;
use super::formatting_configuration::FormattingConfiguration;
use super::producer_policy::ProducerPolicy;
use poirot_functions::text_units::TextSemanticsPolicy;

/// Immutable choices for one calculation: clock/seed, localization, formatting,
/// text semantics, builtin behavior, numeric results, and array/cache publication.
/// Changing this configuration does not require reparsing formulas; cache reuse
/// checks the execution policies and fields relevant to each result.
/// Compilation never captures this policy.
///
/// ```
/// use poirot::eval::{CurrencyContext, EvaluationInputs, FormattingConfiguration, RunConfiguration};
/// let currency = CurrencyContext {
///     decimal_separator: ".".into(), grouping_separator: ",".into(),
///     primary_group: 3, secondary_group: 3,
///     positive_prefix: "$".into(), positive_suffix: "".into(),
///     negative_prefix: "($".into(), negative_suffix: ")".into(),
/// };
/// let configuration = RunConfiguration::new(
///     EvaluationInputs::default(), FormattingConfiguration::with_currency(currency));
/// assert!(configuration.formatting().currency().is_some());
/// ```
///
/// The currency above is deliberately configured by the caller. Default policy
/// leaves currency unspecified; host locale is never read. Pass this policy to
/// `run_formula_with_configuration` or the other configured run entrypoints.
#[derive(Clone, Debug)]
pub struct RunConfiguration {
    context: EvaluationInputs,
    formatting: FormattingConfiguration,
    number_locale: poirot_parsing::number::NumberLocale,
    text_policy: TextSemanticsPolicy,

    builtin_policy: BuiltinSemanticsPolicy,
    array_result_policy: ArrayResultPolicy,
    producer_policy: ProducerPolicy,
    materialization_policy: crate::eval::materialization::FormulaCacheMaterializationPolicy,
}

impl RunConfiguration {
    /// Combine execution inputs and formatting with documented Excel builtins,
    /// Compatibility Version 2 text units, and dynamic worksheet arrays.
    pub fn new(context: EvaluationInputs, formatting: FormattingConfiguration) -> Self {
        Self {
            context,
            formatting,
            number_locale: Default::default(),

            builtin_policy: BuiltinSemanticsPolicy::default(),
            array_result_policy: ArrayResultPolicy::default(),
            producer_policy: ProducerPolicy::default(),
            materialization_policy:
                crate::eval::materialization::FormulaCacheMaterializationPolicy::PreserveComputed,
            text_policy: TextSemanticsPolicy::default(),
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
    /// use poirot::eval::{NumberLocale, RunConfiguration};
    /// let comma = NumberLocale::new(",", Some("."), Some("EUR"), "%", ":")
    ///     .ok_or("invalid numeric locale")?;
    /// let configuration = RunConfiguration::default().with_number_locale(comma);
    /// # Ok::<(), &str>(())
    /// ```
    pub fn with_number_locale(mut self, locale: poirot_parsing::number::NumberLocale) -> Self {
        self.number_locale = locale;
        self
    }

    /// Numeric text input profile. No ambient locale is consulted.
    pub fn number_locale(&self) -> &poirot_parsing::number::NumberLocale {
        &self.number_locale
    }

    /// Select an explicit text source contract.
    pub fn with_text_policy(mut self, text_policy: TextSemanticsPolicy) -> Self {
        self.text_policy = text_policy;
        self
    }

    /// Return the selected text source semantics.
    pub fn text_policy(&self) -> TextSemanticsPolicy {
        self.text_policy
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

    /// Choose computed producers or caller-trusted saved source results.
    pub fn with_producer_policy(mut self, policy: ProducerPolicy) -> Self {
        self.producer_policy = policy;
        self
    }

    /// Return the producer preparation policy.
    pub fn producer_policy(&self) -> ProducerPolicy {
        self.producer_policy
    }

    /// Select conversion behavior for saved workbook formula caches.
    pub fn with_materialization_policy(
        mut self,
        policy: crate::eval::materialization::FormulaCacheMaterializationPolicy,
    ) -> Self {
        self.materialization_policy = policy;
        self
    }

    /// Return the saved-cache conversion policy.
    pub fn materialization_policy(
        &self,
    ) -> crate::eval::materialization::FormulaCacheMaterializationPolicy {
        self.materialization_policy
    }

    /// Complete exact identity, including text source and every owned currency field.
    pub fn fingerprint(&self) -> ConfigurationFingerprint {
        ConfigurationFingerprint {
            context: self.context.fingerprint(),
            formatting: self.formatting.clone(),
            number_locale: self.number_locale.clone(),
            text_policy: self.text_policy,

            builtin_policy: self.builtin_policy,
            array_result_policy: self.array_result_policy,
            producer_policy: self.producer_policy,
            numeric_source: crate::eval::values::numeric_outcome::source_binding(),
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
