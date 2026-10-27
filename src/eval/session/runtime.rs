//! Persistent execution storage, independent of temporarily attached workbook ports.

use super::call_effects::CallEffects;
use super::evaluation_stack::EvaluationStack;
use super::result_memo::ResultMemo;
use crate::eval::limits::{BudgetMeter, RunUsage};
use crate::eval::observe::ObservationState;
use crate::eval::reuse::LookupPrefixes;
use crate::eval::spill::SpillPreparation;
use crate::eval::vm::Machine;

/// Execution storage retained across cell and output calls, composed of one
/// component per concern. A persistent workspace lends it to each session it starts.
pub(in crate::eval) struct Runtime {
    /// Value and operand stacks of the bytecode VM.
    pub(in crate::eval) vm: Machine,
    /// Cumulative work budgets and their usage counters.
    pub(in crate::eval) budget: BudgetMeter,
    /// Successful or failed work reported for the run.
    pub(in crate::eval) run_usage: RunUsage,
    /// Cells and names under evaluation and the consuming target.
    pub(in crate::eval) stack: EvaluationStack,
    /// Memoized cell results and their retention accounting.
    pub(in crate::eval) memo: ResultMemo,
    /// Per-cell random streams, date coercion effects and the extension trace.
    pub(in crate::eval) effects: CallEffects,
    /// Array discovery, provisional producer results and settled spill ownership.
    pub(in crate::eval) preparation: SpillPreparation,
    /// Exact-search prefixes reused across compatible lookup calls.
    pub(in crate::eval) lookup_prefixes: LookupPrefixes,
    /// Observation identities and the currently executing parent operation.
    pub(in crate::eval) observation_state: ObservationState,
}

impl Runtime {
    /// Create empty execution state around a validated cumulative work meter.
    pub(in crate::eval) fn new(budget: BudgetMeter) -> Self {
        Self {
            vm: Default::default(),
            budget,
            run_usage: Default::default(),
            stack: Default::default(),
            memo: Default::default(),
            effects: Default::default(),
            preparation: Default::default(),
            lookup_prefixes: Default::default(),
            observation_state: Default::default(),
        }
    }
}
