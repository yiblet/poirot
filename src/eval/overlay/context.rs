/// Captured execution inputs and resource limits for an evaluation cycle.
/// An overlay supplies the workbook and policies separately, allowing these inputs
/// to change between cycles while retaining unaffected calculated results.
///
/// ```
/// use poirot::eval::{EvaluationContext, EvaluationLimits};
/// let context = EvaluationContext {
///     as_of_time: std::time::UNIX_EPOCH,
///     randomness_seed: 42,
///     limits: EvaluationLimits::default().max_work(10_000),
///     ..Default::default()
/// };
/// assert_eq!(context.limits.work_limit(), Some(10_000));
/// ```
#[derive(Clone, Debug)]
pub struct EvaluationContext {
    /// Fixed clock used by date and time functions during this evaluation.
    /// NOW/TODAY reuse while this and their consumed conversion settings remain
    /// unchanged; changing it invalidates actual clock consumers and dependents.
    pub as_of_time: std::time::SystemTime,
    /// Explicit identity for reproducible per-cell random streams. Reusing the
    /// seed permits reuse of unchanged draws; change it for fresh draws.
    pub randomness_seed: u64,
    /// Date ordering and fixed UTC offset used by localized operations.
    pub localization: crate::eval::Localization,
    /// Formatting inputs consumed by functions that render values as text.
    pub formatting: crate::eval::FormattingConfiguration,
    /// Explicit numeric argument input tokens.
    pub number_locale: crate::eval::NumberLocale,
    /// Limits on work and allocation during this evaluation.
    pub limits: crate::eval::EvaluationLimits,
    /// Native PCRE2 execution guards; not included in spreadsheet cache identity.
    pub regex: crate::eval::RegexSettings,
    /// Limits on memoized values retained by the evaluation.
    pub retention: crate::eval::run::MemoRetentionLimits,
}
impl Default for EvaluationContext {
    fn default() -> Self {
        crate::eval::EvaluationInputs::default().into()
    }
}
impl From<crate::eval::EvaluationInputs> for EvaluationContext {
    /// Combine captured computation inputs with default formatting and execution limits.
    fn from(inputs: crate::eval::EvaluationInputs) -> Self {
        Self {
            as_of_time: inputs.as_of_time,
            randomness_seed: inputs.randomness_seed,
            localization: inputs.localization,
            formatting: Default::default(),
            number_locale: Default::default(),
            limits: Default::default(),
            regex: Default::default(),
            retention: Default::default(),
        }
    }
}
impl EvaluationContext {
    /// Replace calculation quotas without changing clock, randomness, or cache identity.
    /// This configures the next explicit cycle; it never resumes a refused cycle.
    pub fn limits(mut self, limits: crate::eval::EvaluationLimits) -> Self {
        self.limits = limits;
        self
    }
    /// Replace native regex guards, independent of profiling and caller work quotas.
    pub fn regex_settings(mut self, settings: crate::eval::RegexSettings) -> Self {
        self.regex = settings;
        self
    }
    /// Create a context with the current clock and default limits.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the clock, random seed, and localization for reproducible execution.
    pub fn with_clock(
        mut self,
        time: std::time::SystemTime,
        seed: u64,
        localization: crate::eval::Localization,
    ) -> Self {
        self.as_of_time = time;
        self.randomness_seed = seed;
        self.localization = localization;
        self
    }
    /// Capture only clock, seed, and localization for lower-level execution APIs.
    /// Formatting, work budgets, and retention remain settings of this full context.
    pub fn inputs(&self) -> crate::eval::EvaluationInputs {
        crate::eval::EvaluationInputs {
            as_of_time: self.as_of_time,
            randomness_seed: self.randomness_seed,
            localization: self.localization,
        }
    }
    /// Capture context inputs that can affect reusable cached results.
    pub(super) fn key(&self) -> ContextKey {
        ContextKey {
            inputs: self.inputs().fingerprint(),
            formatting: self.formatting.clone(),
            number_locale: self.number_locale.clone(),
        }
    }
}
/// Context identity used to invalidate only results that consumed changed inputs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) struct ContextKey {
    /// Clock, seed, and localization identity.
    pub(super) inputs: crate::eval::ContextFingerprint,
    /// Formatting identity, checked for cells that consumed formatting.
    pub(super) formatting: crate::eval::FormattingConfiguration,
    /// Numeric input token identity for cells that parsed text.
    #[serde(default)]
    pub(super) number_locale: crate::eval::NumberLocale,
}

/// Parsing and computation rules owned by Overlay or a direct Evaluation workspace.
/// Compilation derives a [`crate::eval::PreparationPolicy`] from the parse settings and
/// extension registry; execution choices remain in this policy.
///
/// ```
/// use poirot::eval::ArrayResultPolicy;
/// use poirot::eval::overlay::{Evaluation, EvaluationContext, EvaluationPolicy};
/// use poirot::model::{CellValue, Workbook};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "{2,3}")?;
/// let policy = EvaluationPolicy { arrays: ArrayResultPolicy::DynamicSpill, ..Default::default() };
/// let mut run = Evaluation::with_workbook(&book).with_policy(policy)
///     .build(EvaluationContext::default())?;
/// assert_eq!(run.cell(run.target("Data!B1")?)?, CellValue::Number(3.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluationPolicy {
    /// Formula grammar and preparation limits.
    pub parse: poirot_parsing::formula::FormulaParsePolicy,
    /// Selected builtin compatibility behavior.
    pub builtin: crate::eval::BuiltinSemanticsPolicy,
    /// Rules for returning and publishing array results.
    pub arrays: crate::eval::ArrayResultPolicy,
    /// Compute possible producers or trust saved source results.
    pub producers: crate::eval::ProducerPolicy,
    /// Text comparison and coercion behavior.
    pub text: crate::eval::TextSemanticsPolicy,
    /// Conversion rules when exporting evaluated values into workbook caches.
    pub materialization: crate::eval::FormulaCacheMaterializationPolicy,
}
impl Default for EvaluationPolicy {
    fn default() -> Self {
        let configuration = crate::eval::RunConfiguration::default();
        Self {
            parse: Default::default(),
            builtin: configuration.builtin_policy(),
            arrays: configuration.array_result_policy(),
            producers: configuration.producer_policy(),
            text: configuration.text_policy(),
            materialization: configuration.materialization_policy(),
        }
    }
}
