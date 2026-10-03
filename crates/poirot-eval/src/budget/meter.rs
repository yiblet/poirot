use super::policy::{ChargeRequest, Reservation, WorkSnapshot};
use super::*;

/// Mutable run accounting with nested per-formula work frames.
pub(crate) struct BudgetMeter {
    /// Validated limits applied before each charge or reservation.
    pub limits: EvaluationBudgets,
    /// Accepted work and observed resource usage.
    pub usage: EvaluationUsage,
    depth: usize,
    frames: Vec<(EvaluationTarget, u64)>,
}
impl BudgetMeter {
    /// Validate limits and start empty accounting.
    pub fn new(limits: EvaluationBudgets) -> EvalResult<Self> {
        policy::validate_limits(limits).map_err(EvaluationFailure::Budget)?;
        Ok(Self {
            limits,
            usage: EvaluationUsage::default(),
            depth: 0,
            frames: Vec::new(),
        })
    }
    /// Move run accounting out, leaving an empty meter with the same limits.
    pub(crate) fn take(&mut self) -> Self {
        let replacement = Self {
            limits: self.limits,
            usage: EvaluationUsage::default(),
            depth: 0,
            frames: Vec::new(),
        };
        std::mem::replace(self, replacement)
    }
    fn failure(
        &self,
        kind: BudgetKind,
        limit: u64,
        consumed: u64,
        requested: u64,
    ) -> EvaluationFailure {
        EvaluationFailure::Budget(BudgetFailure {
            kind,
            limit,
            consumed,
            requested,
            target: self.frames.last().map(|(target, _)| *target),
        })
    }
    /// Admit work against run and active-cell limits before updating counters.
    pub fn charge(&mut self, kind: WorkKind, amount: u64) -> EvalResult<()> {
        let ChargeRequest { kind, amount } = policy::admit_work(
            self.limits,
            WorkSnapshot {
                run_consumed: self.usage.work_units,
                cell_consumed: self.frames.last().map(|(_, consumed)| *consumed),
                target: self.frames.last().map(|(target, _)| *target),
            },
            ChargeRequest { kind, amount },
        )
        .map_err(EvaluationFailure::Budget)?;
        self.usage.work_units += amount;
        if let Some((_, consumed)) = self.frames.last_mut() {
            *consumed += amount;
        }
        let counter = match kind {
            WorkKind::Expression => &mut self.usage.expressions,
            WorkKind::CellRead => &mut self.usage.cell_reads,
            WorkKind::SparseScan => &mut self.usage.sparse_entries,
            WorkKind::NamespaceScan => &mut self.usage.namespace_entries,
            WorkKind::Materialization => &mut self.usage.materialized_values,
            WorkKind::Comparison => &mut self.usage.comparisons,
            WorkKind::ScalarOperator => &mut self.usage.scalar_operators,
            WorkKind::HelperOperation => &mut self.usage.helper_operations,
            WorkKind::WildcardState => &mut self.usage.wildcard_states,
            WorkKind::TextByte => &mut self.usage.text_bytes,
            WorkKind::CalendarStep => &mut self.usage.calendar_steps,
            WorkKind::ReferencePair => &mut self.usage.reference_pairs,
        };
        *counter += amount;
        Ok(())
    }
    #[cfg(test)]
    /// Return the smaller remaining run or active-cell work allowance.
    pub fn remaining_work(&self) -> u64 {
        self.limits
            .run_work
            .saturating_sub(self.usage.work_units)
            .min(self.frames.last().map_or(u64::MAX, |(_, consumed)| {
                self.limits.cell_work.saturating_sub(*consumed)
            }))
    }
    /// Admit one nesting level and record peak depth.
    pub fn enter_depth(&mut self) -> EvalResult<()> {
        if self.depth >= self.limits.depth {
            return Err(self.failure(
                BudgetKind::Depth,
                self.limits.depth as u64,
                self.depth as u64,
                1,
            ));
        }
        self.depth += 1;
        self.usage.maximum_depth = self.usage.maximum_depth.max(self.depth);
        Ok(())
    }
    /// Leave one previously admitted nesting level.
    pub fn leave_depth(&mut self) {
        self.depth -= 1;
    }
    /// Start a formula's direct-work frame and failure attribution.
    pub fn push_cell(&mut self, target: EvaluationTarget) {
        self.frames.push((target, 0));
    }
    /// Restore the enclosing formula's direct-work frame.
    pub fn pop_cell(&mut self) {
        self.frames.pop();
    }
    fn admit_reservation(&self, reservation: Reservation) -> EvalResult<()> {
        policy::admit_reservation(
            self.limits,
            self.frames.last().map(|(target, _)| *target),
            reservation,
        )
        .map_err(EvaluationFailure::Budget)
    }
    /// Check a collection size without charging materialization work.
    pub fn check_allocation(&self, count: usize) -> EvalResult<()> {
        self.admit_reservation(Reservation::Values(count as u64))
    }
    /// Admit a collection size and charge its materialized elements.
    pub fn allocation(&mut self, count: usize) -> EvalResult<()> {
        self.check_allocation(count)?;
        self.charge(WorkKind::Materialization, count as u64)
    }
    /// Admit a wide collection count before narrowing and charging it.
    pub fn allocation_u64(&mut self, count: u64) -> EvalResult<()> {
        self.admit_reservation(Reservation::Values(count))?;
        self.allocation(count as usize)
    }
    /// Admit text-buffer capacity and charge text processing work.
    pub fn text(&mut self, bytes: usize) -> EvalResult<()> {
        self.admit_reservation(Reservation::TextBytes(bytes))?;
        self.charge(WorkKind::TextByte, bytes as u64)
    }
    /// Admit per-match wildcard state count and charge matching work.
    pub fn wildcard(&mut self, states: u64) -> EvalResult<()> {
        self.admit_reservation(Reservation::WildcardStates(states))?;
        self.charge(WorkKind::WildcardState, states)
    }
    /// Finish accounting with usage attached to success or operational failure.
    pub fn outcome<T>(self, result: EvalResult<T>) -> EvaluationOutcome<T> {
        match result {
            Ok(value) => EvaluationOutcome::Complete {
                value,
                usage: self.usage,
            },
            Err(failure) => EvaluationOutcome::Aborted {
                failure,
                usage: self.usage,
            },
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
