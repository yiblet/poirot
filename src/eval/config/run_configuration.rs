use super::array_result_policy::ArrayResultPolicy;
use super::configuration_fingerprint::ConfigurationFingerprint;
use super::evaluation_inputs::EvaluationInputs;
use super::formatting_configuration::FormattingConfiguration;
use super::producer_policy::ProducerPolicy;

/// Immutable inputs for one calculation: clock, seed, localization, formatting,
/// numeric input locale, array publication and producer handling.
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

    array_result_policy: ArrayResultPolicy,
    producer_policy: ProducerPolicy,
}

impl RunConfiguration {
    /// Combine execution inputs and formatting with documented Excel builtins,
    /// Compatibility Version 2 text units, and dynamic worksheet arrays.
    pub fn new(context: EvaluationInputs, formatting: FormattingConfiguration) -> Self {
        Self {
            context,
            formatting,
            number_locale: Default::default(),

            array_result_policy: ArrayResultPolicy::default(),
            producer_policy: ProducerPolicy::default(),
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
    /// let comma = NumberLocale::new(",", Some("."), Some("EUR"), "%", ":")?;
    /// let configuration = RunConfiguration::default().with_number_locale(comma);
    /// # Ok::<(), poirot_parsing::number::NumberLocaleError>(())
    /// ```
    pub fn with_number_locale(mut self, locale: poirot_parsing::number::NumberLocale) -> Self {
        self.number_locale = locale;
        self
    }

    /// Numeric text input profile. No ambient locale is consulted.
    pub fn number_locale(&self) -> &poirot_parsing::number::NumberLocale {
        &self.number_locale
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

    /// Complete exact identity, including text source and every owned currency field.
    pub fn fingerprint(&self) -> ConfigurationFingerprint {
        ConfigurationFingerprint {
            context: self.context.fingerprint(),
            formatting: self.formatting.clone(),
            number_locale: self.number_locale.clone(),

            array_result_policy: self.array_result_policy,
            producer_policy: self.producer_policy,
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
