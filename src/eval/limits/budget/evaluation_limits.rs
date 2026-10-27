use super::evaluation_budgets::EvaluationBudgets;

/// Reusable, opt-in calculation quotas, independent of profiling and semantic context.
///
/// Work is cumulative until an explicit new cycle; formula work excludes dependencies.
/// Collection values and text bytes are checked before each logical allocation or cached
/// result admission, not against total heap usage. No quotas are configured by default.
/// Nesting retains the engine's 128-frame safety ceiling.
///
/// ```
/// use poirot::eval::EvaluationLimits;
/// let limits = EvaluationLimits::default()
///     .max_work(100_000)
///     .max_text_bytes(1_024)
///     .max_depth(64);
/// assert_eq!(limits.work_limit(), Some(100_000));
/// assert_eq!(limits.without_work_limit().work_limit(), None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationLimits {
    pub(super) work: Option<u64>,
    pub(super) formula_work: Option<u64>,
    pub(super) collection_values: Option<usize>,
    pub(super) text_bytes: Option<usize>,
    pub(super) wildcard_states: Option<u64>,
    pub(super) depth: Option<usize>,
}

impl EvaluationLimits {
    /// Set cumulative work units per explicit cycle; zero allows none. Checked before the operation.
    pub fn max_work(mut self, maximum: u64) -> Self {
        self.work = Some(maximum);
        self
    }

    /// Remove the work quota. Does not reset an existing cycle.
    pub fn without_work_limit(mut self) -> Self {
        self.work = None;
        self
    }

    /// Inspect cumulative work units per explicit cycle; `None` means no application quota.
    pub fn work_limit(&self) -> Option<u64> {
        self.work
    }

    /// Set direct work units per formula; zero allows none. Checked before the operation.
    pub fn max_formula_work(mut self, maximum: u64) -> Self {
        self.formula_work = Some(maximum);
        self
    }

    /// Remove the formula work quota. Does not reset an existing cycle.
    pub fn without_formula_work_limit(mut self) -> Self {
        self.formula_work = None;
        self
    }

    /// Inspect direct work units per formula; `None` means no application quota.
    pub fn formula_work_limit(&self) -> Option<u64> {
        self.formula_work
    }

    /// Set values per logical collection; zero allows none. Checked before the operation.
    pub fn max_collection_values(mut self, maximum: usize) -> Self {
        self.collection_values = Some(maximum);
        self
    }

    /// Remove the collection values quota. Does not reset an existing cycle.
    pub fn without_collection_values_limit(mut self) -> Self {
        self.collection_values = None;
        self
    }

    /// Inspect values per logical collection; `None` means no application quota.
    pub fn collection_values_limit(&self) -> Option<usize> {
        self.collection_values
    }

    /// Set bytes per logical text buffer; zero allows none. Checked before the operation.
    pub fn max_text_bytes(mut self, maximum: usize) -> Self {
        self.text_bytes = Some(maximum);
        self
    }

    /// Remove the text bytes quota. Does not reset an existing cycle.
    pub fn without_text_bytes_limit(mut self) -> Self {
        self.text_bytes = None;
        self
    }

    /// Inspect bytes per logical text buffer; `None` means no application quota.
    pub fn text_bytes_limit(&self) -> Option<usize> {
        self.text_bytes
    }

    /// Set visited states per wildcard match; zero allows none. Checked before the operation.
    pub fn max_wildcard_states(mut self, maximum: u64) -> Self {
        self.wildcard_states = Some(maximum);
        self
    }

    /// Remove the wildcard states quota. Does not reset an existing cycle.
    pub fn without_wildcard_states_limit(mut self) -> Self {
        self.wildcard_states = None;
        self
    }

    /// Inspect visited states per wildcard match; `None` means no application quota.
    pub fn wildcard_states_limit(&self) -> Option<u64> {
        self.wildcard_states
    }

    /// Set nested evaluation frames, at most 128; zero allows none. Checked before the operation.
    pub fn max_depth(mut self, maximum: usize) -> Self {
        self.depth = Some(maximum);
        self
    }

    /// Remove the depth quota. Does not reset an existing cycle.
    pub fn without_depth_limit(mut self) -> Self {
        self.depth = None;
        self
    }

    /// Inspect nested evaluation frames, at most 128; `None` means no application quota.
    pub fn depth_limit(&self) -> Option<usize> {
        self.depth
    }
}

impl From<EvaluationBudgets> for EvaluationLimits {
    fn from(budgets: EvaluationBudgets) -> Self {
        Self {
            work: (budgets.run_work != u64::MAX).then_some(budgets.run_work),
            formula_work: (budgets.cell_work != u64::MAX).then_some(budgets.cell_work),
            depth: (budgets.depth != 128).then_some(budgets.depth),
            collection_values: (budgets.materialized_values != usize::MAX)
                .then_some(budgets.materialized_values),
            text_bytes: (budgets.text_bytes != usize::MAX).then_some(budgets.text_bytes),
            wildcard_states: (budgets.wildcard_states != u64::MAX)
                .then_some(budgets.wildcard_states),
        }
    }
}
