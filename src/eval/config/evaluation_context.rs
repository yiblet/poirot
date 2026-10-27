use super::context_key::ContextKey;
use super::evaluation_inputs::EvaluationInputs;
use super::formatting_configuration::FormattingConfiguration;
use super::localization::Localization;

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
    pub localization: Localization,
    /// Formatting inputs consumed by functions that render values as text.
    pub formatting: FormattingConfiguration,
    /// Explicit numeric argument input tokens.
    pub number_locale: poirot_parsing::number::NumberLocale,
    /// Limits on work and allocation during this evaluation.
    pub limits: crate::eval::limits::EvaluationLimits,
    /// Native PCRE2 execution guards; not included in spreadsheet cache identity.
    pub regex: poirot_functions::text::regex::ExecutionLimits,
    /// Limits on memoized values retained by the evaluation.
    pub retention: crate::eval::limits::MemoRetentionLimits,
}

impl Default for EvaluationContext {
    fn default() -> Self {
        EvaluationInputs::default().into()
    }
}

impl From<EvaluationInputs> for EvaluationContext {
    /// Combine captured computation inputs with default formatting and execution limits.
    fn from(inputs: EvaluationInputs) -> Self {
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
    pub fn limits(mut self, limits: crate::eval::limits::EvaluationLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Replace native regex guards, independent of profiling and caller work quotas.
    pub fn regex_settings(
        mut self,
        settings: poirot_functions::text::regex::ExecutionLimits,
    ) -> Self {
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
        localization: Localization,
    ) -> Self {
        self.as_of_time = time;
        self.randomness_seed = seed;
        self.localization = localization;
        self
    }

    /// Capture only clock, seed, and localization for lower-level execution APIs.
    /// Formatting, work budgets, and retention remain settings of this full context.
    pub fn inputs(&self) -> EvaluationInputs {
        EvaluationInputs {
            as_of_time: self.as_of_time,
            randomness_seed: self.randomness_seed,
            localization: self.localization,
        }
    }

    /// Capture context inputs that can affect reusable cached results.
    pub(in crate::eval) fn key(&self) -> ContextKey {
        ContextKey {
            inputs: self.inputs().fingerprint(),
            formatting: self.formatting.clone(),
            number_locale: self.number_locale.clone(),
        }
    }
}
