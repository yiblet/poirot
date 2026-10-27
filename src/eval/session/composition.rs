use super::reuse_handles::ReuseHandles;
use super::run_policies::RunPolicies;
use super::run_ports::RunPorts;
use super::run_sources::RunSources;
use super::run_state::RunState;
use super::runtime::Runtime;
use super::storage::Storage;
use crate::eval::config::EvaluationInputs;
use crate::eval::limits::BudgetMeter;
#[cfg(test)]
use crate::eval::limits::EvaluationBudgets;
use crate::model::Workbook;

/// Mutable execution state shared by nested formulas within one bounded run.
pub(in crate::eval) struct Session<'a> {
    /// Workbook content and compilation services the run reads from.
    pub(in crate::eval) sources: RunSources<'a>,
    /// Execution policies selected for the run.
    pub(in crate::eval) policies: RunPolicies<'a>,
    /// Execution storage and the active expression mode.
    pub(in crate::eval) state: RunState<'a>,
    /// Overlay handles for incremental reuse and dependency recording.
    pub(in crate::eval) reuse: ReuseHandles<'a>,
    /// Caller-installed observation ports.
    pub(in crate::eval) ports: RunPorts<'a>,
}

impl<'a> Session<'a> {
    /// Start a test session with explicit context and default work limits.
    #[cfg(test)]
    pub(in crate::eval) fn with_context(workbook: &'a Workbook, context: EvaluationInputs) -> Self {
        Self::new(
            workbook,
            context,
            BudgetMeter::new(EvaluationBudgets::default()).unwrap(),
        )
    }

    /// Create empty run state around a borrowed workbook and validated work meter.
    pub(in crate::eval) fn new(
        workbook: &'a Workbook,
        context: EvaluationInputs,
        budget: BudgetMeter,
    ) -> Self {
        Self::with_configuration(workbook, RunPolicies::from_inputs(context), budget)
    }

    /// Initialize owned execution storage with borrowed configuration settings.
    pub(in crate::eval) fn with_configuration(
        workbook: &'a Workbook,
        configuration: RunPolicies<'a>,
        budget: BudgetMeter,
    ) -> Self {
        Self::with_runtime(
            workbook,
            configuration,
            // SAFETY: A single fixed-size Runtime has a representable allocation layout.
            Storage::Owned(Box::new(Runtime::new(budget))),
        )
    }

    /// Attach workbook ports to owned or persistently borrowed execution storage.
    pub(in crate::eval) fn with_runtime(
        workbook: &'a Workbook,
        configuration: RunPolicies<'a>,
        runtime: Storage<'a>,
    ) -> Self {
        Self {
            sources: RunSources::new(workbook),
            policies: configuration,
            state: RunState::new(runtime),
            reuse: ReuseHandles::default(),
            ports: RunPorts::default(),
        }
    }
}
