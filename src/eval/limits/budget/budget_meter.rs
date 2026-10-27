use super::budget_failure::BudgetFailure;
use super::budget_kind::BudgetKind;
use super::capability_id::CapabilityId;
use super::charge_request::ChargeRequest;
use super::eval_result::EvalResult;
use super::evaluation_budgets::EvaluationBudgets;
use super::evaluation_failure::EvaluationFailure;
use super::evaluation_limits::EvaluationLimits;
use super::evaluation_outcome::EvaluationOutcome;
use super::evaluation_usage::EvaluationUsage;
use super::policy;
use super::reservation::Reservation;
use super::resource_failure_kind::ResourceFailureKind;
use super::work_kind::WorkKind;
use super::work_snapshot::WorkSnapshot;
use crate::eval::coordinates::EvaluationTarget;

/// Mutable run accounting with nested per-formula work frames.
pub(in crate::eval) struct BudgetMeter {
    /// Validated limits applied before each charge or reservation.
    pub limits: EvaluationBudgets,
    /// Accepted work and observed resource usage.
    pub usage: EvaluationUsage,
    pub(in crate::eval) profiling: bool,
    run_limited: bool,
    cell_limited: bool,
    depth_limited: bool,
    /// Current nested formula evaluation depth.
    pub(super) depth: usize,
    /// Active per-formula work frames: target and work consumed by that formula.
    pub(super) frames: Vec<(EvaluationTarget, u64)>,
}

impl BudgetMeter {
    /// Validate limits and start empty accounting.
    pub fn new(limits: EvaluationBudgets) -> EvalResult<Self> {
        Self::configured(limits, true)
    }

    /// Start accounting only for enabled quotas; detailed counters remain opt-in.
    pub(in crate::eval) fn configured(
        limits: EvaluationBudgets,
        profiling: bool,
    ) -> EvalResult<Self> {
        policy::validate_limits(limits).map_err(EvaluationFailure::Budget)?;
        Ok(Self {
            limits,
            profiling,
            run_limited: limits.run_work != u64::MAX,
            cell_limited: limits.cell_work != u64::MAX,
            depth_limited: true,
            usage: EvaluationUsage::default(),
            depth: 0,
            frames: Vec::new(),
        })
    }

    /// Retain explicit maximum-valued quotas instead of interpreting them as legacy sentinels.
    pub(in crate::eval) fn with_optional_limits(mut self, limits: EvaluationLimits) -> Self {
        self.run_limited = limits.work_limit().is_some();
        self.cell_limited = limits.formula_work_limit().is_some();
        self.depth_limited = limits.depth_limit().is_some();
        self
    }

    /// Move run accounting out, leaving an empty meter with the same limits.
    pub(in crate::eval) fn take(&mut self) -> Self {
        let replacement = Self {
            limits: self.limits,
            profiling: self.profiling,
            run_limited: self.run_limited,
            cell_limited: self.cell_limited,
            depth_limited: self.depth_limited,
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
    #[inline]
    pub fn charge(&mut self, kind: WorkKind, amount: u64) -> EvalResult<()> {
        if amount == 0 || (!self.profiling && !self.run_limited && !self.cell_limited) {
            return Ok(());
        }
        let ChargeRequest { kind, amount } = policy::admit_work(
            self.limits,
            WorkSnapshot {
                run_limited: self.run_limited,
                cell_limited: self.cell_limited,
                run_consumed: self.usage.work_units,
                cell_consumed: self.frames.last().map(|(_, consumed)| *consumed),
                target: self.frames.last().map(|(target, _)| *target),
            },
            ChargeRequest { kind, amount },
        )
        .map_err(EvaluationFailure::Budget)?;
        self.usage.work_units = self.usage.work_units.saturating_add(amount);
        if let Some((_, consumed)) = self.frames.last_mut() {
            *consumed = consumed.saturating_add(amount);
        }
        if !self.profiling {
            return Ok(());
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
        *counter = counter.saturating_add(amount);
        Ok(())
    }

    #[cfg(test)]
    /// Return the smaller remaining run or active-cell work allowance.
    pub fn remaining_work(&self) -> u64 {
        let run = if self.limits.run_work == u64::MAX {
            u64::MAX
        } else {
            self.limits.run_work.saturating_sub(self.usage.work_units)
        };
        let cell = self.frames.last().map_or(u64::MAX, |(_, consumed)| {
            if self.limits.cell_work == u64::MAX {
                u64::MAX
            } else {
                self.limits.cell_work.saturating_sub(*consumed)
            }
        });
        run.min(cell)
    }

    /// Admit one nesting level and record peak depth.
    pub fn enter_depth(&mut self) -> EvalResult<()> {
        if self.depth >= self.limits.depth {
            if !self.depth_limited {
                return Err(EvaluationFailure::Resource {
                    kind: ResourceFailureKind::BackendGuard,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.frames.last().map(|(target, _)| *target),
                    detail: "evaluation reached the engine's 128-frame nesting ceiling".into(),
                });
            }
            return Err(self.failure(
                BudgetKind::Depth,
                self.limits.depth as u64,
                self.depth as u64,
                1,
            ));
        }
        self.depth += 1;
        if self.profiling {
            self.usage.maximum_depth = self.usage.maximum_depth.max(self.depth);
        }
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

    /// Start a cell frame after reserving its storage without panicking on allocation failure.
    pub fn try_push_cell(&mut self, target: EvaluationTarget) -> EvalResult<()> {
        self.frames
            .try_reserve(1)
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::eval::limits::budget::resource_failure_kind::ResourceFailureKind::Allocation,
                capability: crate::eval::limits::budget::capability_id::CapabilityId::FunctionAdapter,
                source: Some(target),
                detail: "cell budget frame allocation failed".into(),
            })?;
        // SAFETY: The successful reservation provides one frame slot.
        self.frames.push((target, 0));
        Ok(())
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

    /// Admit text-buffer capacity when incremental copies charge work separately.
    pub fn check_text(&self, bytes: usize) -> EvalResult<()> {
        self.admit_reservation(Reservation::TextBytes(bytes))
    }

    /// Admit text-buffer capacity and charge text processing work.
    pub fn text(&mut self, bytes: usize) -> EvalResult<()> {
        self.check_text(bytes)?;
        self.charge(WorkKind::TextByte, bytes as u64)
    }

    /// Admit per-match wildcard state count and charge matching work.
    pub fn wildcard(&mut self, states: u64) -> EvalResult<()> {
        self.admit_reservation(Reservation::WildcardStates(states))?;
        self.charge(WorkKind::WildcardState, states)
    }

    /// Admit one visited wildcard state against a caller-owned per-match counter.
    pub fn wildcard_step(&mut self, visited: &mut u64) -> EvalResult<()> {
        let next = visited.saturating_add(1);
        if next > self.limits.wildcard_states {
            return Err(self.failure(
                BudgetKind::WildcardStates,
                self.limits.wildcard_states,
                *visited,
                1,
            ));
        }
        self.charge(WorkKind::WildcardState, 1)?;
        *visited = next;
        Ok(())
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
