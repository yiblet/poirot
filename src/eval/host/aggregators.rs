//! Lazy contextual aggregation projection and scoped alias resolution.
use super::call_host::CallHost;
use crate::eval::compile::bytecode::aggregation::{Candidate, Plan};
use crate::eval::limits::EvaluationError;
use crate::eval::ports::DependencyRead;
use crate::eval::session::{NameResolver, ReadRecorder};
use crate::eval::values::{FunctionResult, identifier_failure};
use crate::model::CellError;
use poirot_functions::statistics::BuiltinAggregator;

/// Prepared built-in aggregation handlers for grouping kernels.
/// The scalar or one-axis vector shape preserves the aggregation argument's layout
/// without converting handler identities into worksheet values.
pub(in crate::eval) struct Aggregators {
    /// Vertical aggregation count.
    pub(in crate::eval) rows: usize,
    /// Horizontal aggregation count.
    pub(in crate::eval) columns: usize,
    /// Row-major built-in identities.
    pub(in crate::eval) members: Vec<BuiltinAggregator>,
}

impl Aggregators {
    /// Project a deferred aggregation role without evaluating it as worksheet data.
    pub(in crate::eval) fn resolve(
        host: &mut CallHost<'_, '_>,
        index: usize,
    ) -> FunctionResult<Self> {
        let plan = host
            .arguments
            .get(index)
            .ok_or(CellError::Value)?
            .aggregation()
            .map_err(|_| host.budget_exceeded())?
            .ok_or(CellError::Value)?;
        Self::resolve_plan(host, &plan, 0)
    }

    /// Resolve one lowered aggregation plan, following named aliases to a bounded depth.
    fn resolve_plan(
        host: &mut CallHost<'_, '_>,
        plan: &Plan,
        depth: usize,
    ) -> FunctionResult<Self> {
        if depth >= crate::eval::compile::bytecode::DEPTH || !plan.valid() {
            return Err(CellError::Value.into());
        }
        host.check_values(plan.members.len())?;
        let mut members = Vec::new();
        let mut rows = plan.rows;
        let mut columns = plan.columns;
        for candidate in &plan.members {
            host.charge_work(1)?;
            let projected = Self::resolve_candidate(host, candidate, depth.saturating_add(1))?;
            if plan.members.len() == 1 {
                rows = projected.rows;
                columns = projected.columns;
            } else if (plan.rows == 1 && projected.rows != 1)
                || (plan.columns == 1 && projected.columns != 1)
            {
                return Err(CellError::Value.into());
            }
            let count = members
                .len()
                .checked_add(projected.members.len())
                .ok_or_else(|| host.budget_exceeded())?;
            if count > 100_000 {
                return Err(CellError::Value.into());
            }
            host.check_values_capacity(count)?;
            host.check_values(projected.members.len())?;
            members
                .try_reserve(projected.members.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: The preceding successful reservation admits this unchanged projected member count.
            members.extend(projected.members);
        }
        if plan.members.len() > 1 {
            if rows == 1 {
                columns = members.len();
            } else {
                rows = members.len();
            }
        }
        Ok(Self {
            rows,
            columns,
            members,
        })
    }

    /// Resolve one candidate as a defined-name alias or a builtin aggregator name.
    fn resolve_candidate(
        host: &mut CallHost<'_, '_>,
        candidate: &Candidate,
        depth: usize,
    ) -> FunctionResult<Aggregators> {
        let scope = NameResolver::new(host.session)
            .sheet_named(host.sheet, candidate.sheet.as_deref())?
            .ok_or(CellError::Ref)?;
        host.check_text(candidate.name.len())?;
        host.charge_work(
            (host.session.sources.workbook.named_ranges.len() as u64).saturating_mul(2),
        )?;
        let selected = host
            .session
            .sources
            .workbook
            .try_defined_name(&candidate.name, scope)
            .map_err(identifier_failure)?
            .and_then(|name| {
                host.session
                    .sources
                    .workbook
                    .named_ranges
                    .iter()
                    .position(|n| std::ptr::eq(n, name))
            });
        ReadRecorder::new(host.session).read_lazy(|| DependencyRead::Name {
            sheet: scope,
            name: candidate.name.clone(),
            selected,
        });
        if let Some(index) = selected {
            host.check_values(
                host.session
                    .state
                    .runtime
                    .stack
                    .names
                    .len()
                    .saturating_add(1),
            )?;
            let key = (host.sheet, index);
            host.session
                .state
                .runtime
                .stack
                .names
                .try_reserve(1)
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: The successful local reservation admits this one name-cycle key.
            if !host.session.state.runtime.stack.names.insert(key) {
                return Err(CellError::Ref.into());
            }
            let result = (|| {
                let formula = &host
                    .session
                    .sources
                    .workbook
                    .named_ranges
                    .get(index)
                    .ok_or(EvaluationError::StalePlan)?
                    .formula;
                host.session.state.runtime.budget.text(formula.len())?;
                let syntax = poirot_parsing::formula::parse_formula(formula)
                    .map_err(|_| CellError::Value)?;
                let plan = crate::eval::compile::bytecode::aggregation::lower_with_registry(
                    &syntax,
                    host.session.sources.extensions,
                )
                .map_err(|_| host.budget_exceeded())?
                .ok_or(CellError::Value)?;
                Self::resolve_plan(host, &plan, depth)
            })();
            host.session.state.runtime.stack.names.remove(&key);
            return result;
        }
        if candidate.sheet.is_some() {
            return Err(CellError::Name.into());
        }
        let operation = BuiltinAggregator::from_name(&candidate.name).ok_or(CellError::Name)?;
        let mut members = Vec::new();
        members
            .try_reserve_exact(1)
            .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
        // SAFETY: One slot was reserved locally and the vector is still empty.
        members.push(operation);
        Ok(Self {
            rows: 1,
            columns: 1,
            members,
        })
    }
}
