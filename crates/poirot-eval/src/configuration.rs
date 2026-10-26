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
/// Owned user-locale descriptor: no leaked or assumed static locale strings.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CurrencyContext {
    /// Separator between integer and fractional digits.
    pub decimal_separator: String,
    /// Separator inserted between integer digit groups.
    pub grouping_separator: String,
    /// Size of the rightmost integer digit group.
    pub primary_group: usize,
    /// Size of subsequent integer digit groups.
    pub secondary_group: usize,
    /// Affix preceding nonnegative amounts.
    pub positive_prefix: String,
    /// Affix following nonnegative amounts.
    pub positive_suffix: String,
    /// Affix preceding negative amounts.
    pub negative_prefix: String,
    /// Affix following negative amounts.
    pub negative_suffix: String,
}
impl CurrencyContext {
    /// Borrow these owned settings as the financial helper's formatting descriptor.
    pub fn as_format(&self) -> poirot_functions_financial::CurrencyFormat<'_> {
        poirot_functions_financial::CurrencyFormat {
            decimal_separator: &self.decimal_separator,
            grouping_separator: &self.grouping_separator,
            primary_group: self.primary_group,
            secondary_group: self.secondary_group,
            positive_prefix: &self.positive_prefix,
            positive_suffix: &self.positive_suffix,
            negative_prefix: &self.negative_prefix,
            negative_suffix: &self.negative_suffix,
        }
    }
}
impl From<poirot_functions_financial::CurrencyFormat<'_>> for CurrencyContext {
    fn from(f: poirot_functions_financial::CurrencyFormat<'_>) -> Self {
        Self {
            decimal_separator: f.decimal_separator.into(),
            grouping_separator: f.grouping_separator.into(),
            primary_group: f.primary_group,
            secondary_group: f.secondary_group,
            positive_prefix: f.positive_prefix.into(),
            positive_suffix: f.positive_suffix.into(),
            negative_prefix: f.negative_prefix.into(),
            negative_suffix: f.negative_suffix.into(),
        }
    }
}
use crate::{ContextFingerprint, EvaluationInputs};
use poirot_text::TextSemanticsPolicy;

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
    PinnedPoi,
    /// Documented Excel contracts. This is not an Excel observation/parity claim.
    #[default]
    ExcelDocumentation,
}

/// Worksheet publication of final array results, independent of builtin semantics.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ArrayResultPolicy {
    /// Reduce final arrays to a scalar using intersection behavior.
    ScalarIntersection,
    /// Prepare intersecting possible spill rectangles and their necessary competitors.
    /// Publish arrays and area-reference roots atomically with ownership and blocker checks.
    /// Accepted one-cell arrays retain a footprint for postfix spill references (`#`).
    /// Actual overlaps always favor the top-left anchor (row, then column).
    /// Dependency components settle privately; operational failures poison the evaluation.
    #[default]
    DynamicSpill,
}

/// Whether possible dynamic producers execute or consume caller-trusted saved results.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ProducerPolicy {
    /// Compute demanded spill producers and their necessary competitors.
    #[default]
    Compute,
    /// Under DynamicSpill, consume saved source results without executing possible producers.
    /// Ordinary scalar formulas still execute. Missing scalar caches are indistinguishable
    /// from explicit Blank; both remain Blank. Complete declared arrays restore atomically;
    /// incomplete, undeclared owned, or child-edited arrays fail without recalculation.
    /// Replacing the anchor's content discards its baseline saved footprint.
    /// Saved values can deliberately remain stale across source and context changes.
    TrustSaved,
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

    builtin_policy: BuiltinSemanticsPolicy,
    array_result_policy: ArrayResultPolicy,
    producer_policy: ProducerPolicy,
    materialization_policy: crate::FormulaCacheMaterializationPolicy,
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
            materialization_policy: crate::FormulaCacheMaterializationPolicy::PreserveComputed,
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
        policy: crate::FormulaCacheMaterializationPolicy,
    ) -> Self {
        self.materialization_policy = policy;
        self
    }
    /// Return the saved-cache conversion policy.
    pub fn materialization_policy(&self) -> crate::FormulaCacheMaterializationPolicy {
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
            numeric_source: crate::numeric_outcome::source_binding(),
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

    /// Selected builtin compatibility behavior.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Selected array publication behavior.
    pub array_result_policy: ArrayResultPolicy,
    /// Producer computation or caller-trusted saved-source behavior.
    pub producer_policy: ProducerPolicy,
    /// Pinned numeric source identity, when selected.
    pub numeric_source: crate::NumericSourceBinding,
    /// Saved-cache conversion identity.
    pub materialization: crate::FormulaCacheMaterializationIdentity,
}

/// Core computation projection. Consumed formatting/text/context remain separately
/// guarded by the existing cache masks; producer trust changes the result source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputationFingerprint {
    /// Numeric result behavior affecting raw computation.

    /// Builtin compatibility behavior affecting raw computation.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Array publication behavior affecting computed results.
    pub array_result_policy: ArrayResultPolicy,
    /// Producer computation or caller-trusted saved-source behavior.
    pub producer_policy: ProducerPolicy,
    /// Pinned numeric implementation identity, when selected.
    pub numeric_source: crate::NumericSourceBinding,
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

/// Borrowed execution settings shared by every internal session constructor.
pub(crate) struct SessionConfiguration<'a> {
    pub(super) context: EvaluationInputs,
    pub(super) currency: Option<&'a CurrencyContext>,
    pub(super) number_locale: &'a crate::NumberLocaleV2,
    pub(super) text_policy: TextSemanticsPolicy,
    pub(super) builtin_policy: BuiltinSemanticsPolicy,
    pub(super) array_result_policy: ArrayResultPolicy,
    pub(super) producer_policy: ProducerPolicy,
    pub(super) regex: crate::RegexSettings,
}
impl<'a> SessionConfiguration<'a> {
    /// Select default execution policies for captured inputs.
    pub(crate) fn from_inputs(context: EvaluationInputs) -> Self {
        Self {
            context,
            currency: None,
            number_locale: &crate::NumberLocaleV2::EN_US,
            text_policy: TextSemanticsPolicy::default(),
            builtin_policy: BuiltinSemanticsPolicy::default(),
            array_result_policy: ArrayResultPolicy::default(),
            producer_policy: ProducerPolicy::default(),
            regex: Default::default(),
        }
    }

    /// Borrow localization and formatting from an explicit run configuration.
    pub(crate) fn from_run_configuration(configuration: &'a RunConfiguration) -> Self {
        Self {
            context: configuration.context(),
            currency: configuration.formatting().currency(),
            number_locale: configuration.number_locale(),
            text_policy: configuration.text_policy(),
            builtin_policy: configuration.builtin_policy(),
            array_result_policy: configuration.array_result_policy(),
            producer_policy: configuration.producer_policy(),
            regex: Default::default(),
        }
    }

    /// Combine per-evaluation inputs with policies owned by the evaluation workspace.
    pub(crate) fn from_overlay(
        context: &'a crate::EvaluationContext,
        policy: &crate::overlay::OverlayPolicy,
    ) -> Self {
        Self {
            context: context.inputs(),
            currency: context.formatting.currency(),
            number_locale: &context.number_locale,
            text_policy: policy.text,
            builtin_policy: policy.builtin,
            array_result_policy: policy.arrays,
            producer_policy: policy.producers,
            regex: context.regex,
        }
    }
}
