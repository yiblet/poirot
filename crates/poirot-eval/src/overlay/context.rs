/// Per-evaluation inputs. No workbook binding, edits, compiler, policies, or result cache.
#[derive(Clone, Debug)]
pub struct EvaluationContext {
    /// Fixed clock used by date and time functions during this evaluation.
    pub as_of_time: std::time::SystemTime,
    /// Explicit identity for reproducible per-cell random streams. Reusing the
    /// seed reproduces draws after volatile refresh; change it for fresh draws.
    pub randomness_seed: u64,
    /// Date ordering and fixed UTC offset used by localized operations.
    pub localization: crate::Localization,
    /// Formatting inputs consumed by functions that render values as text.
    pub formatting: crate::FormattingConfiguration,
    /// Explicit numeric argument input tokens.
    pub number_locale: crate::NumberLocaleV2,
    /// Limits on work and allocation during this evaluation.
    pub budgets: crate::EvaluationBudgets,
    /// Limits on memoized values retained by the evaluation.
    pub retention: crate::run::MemoRetentionLimits,
}
impl Default for EvaluationContext {
    fn default() -> Self {
        crate::EvaluationInputs::default().into()
    }
}
impl From<crate::EvaluationInputs> for EvaluationContext {
    /// Combine captured computation inputs with default formatting and execution limits.
    fn from(inputs: crate::EvaluationInputs) -> Self {
        Self {
            as_of_time: inputs.as_of_time,
            randomness_seed: inputs.randomness_seed,
            localization: inputs.localization,
            formatting: Default::default(),
            number_locale: Default::default(),
            budgets: Default::default(),
            retention: Default::default(),
        }
    }
}
impl EvaluationContext {
    /// Create a context with the current clock and default limits.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set the clock, random seed, and localization for reproducible execution.
    pub fn with_clock(
        mut self,
        time: std::time::SystemTime,
        seed: u64,
        localization: crate::Localization,
    ) -> Self {
        self.as_of_time = time;
        self.randomness_seed = seed;
        self.localization = localization;
        self
    }
    /// Capture only clock, seed, and localization for lower-level execution APIs.
    /// Formatting, work budgets, and retention remain settings of this full context.
    pub fn inputs(&self) -> crate::EvaluationInputs {
        crate::EvaluationInputs {
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
pub(crate) struct ContextKey {
    /// Clock, seed, and localization identity.
    pub(super) inputs: crate::ContextFingerprint,
    /// Formatting identity, checked for cells that consumed formatting.
    pub(super) formatting: crate::FormattingConfiguration,
    /// Numeric input token identity for cells that parsed text.
    #[serde(default)]
    pub(super) number_locale: crate::NumberLocaleV2,
}

/// Parsing and computation rules owned by Overlay or a direct Evaluation workspace.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OverlayPolicy {
    /// Formula grammar and preparation limits.
    pub parse: poirot_parsing::formula::FormulaParsePolicy,
    /// Selected builtin compatibility behavior.
    pub builtin: crate::BuiltinSemanticsPolicy,
    /// Rules for returning and publishing array results.
    pub arrays: crate::ArrayResultPolicy,
    /// Text comparison and coercion behavior.
    pub text: crate::TextSemanticsPolicy,
    /// Conversion rules when exporting evaluated values into workbook caches.
    pub materialization: crate::FormulaCacheMaterializationPolicy,
}
impl Default for OverlayPolicy {
    fn default() -> Self {
        let configuration = crate::RunConfiguration::default();
        Self {
            parse: Default::default(),
            builtin: configuration.builtin_policy(),
            arrays: configuration.array_result_policy(),
            text: configuration.text_policy(),
            materialization: configuration.materialization_policy(),
        }
    }
}
