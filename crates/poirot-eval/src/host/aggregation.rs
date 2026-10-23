//! Lazy contextual aggregation projection and scoped alias resolution.
use super::*;
use crate::bytecode::aggregation::{Candidate, Plan};
use poirot_functions_statistics::BuiltinAggregator;

/// Resolved scalar or one-axis vector of admitted built-in aggregation handles.
pub(crate) struct Aggregators {
    /// Vertical aggregation count.
    pub(crate) rows: usize,
    /// Horizontal aggregation count.
    pub(crate) columns: usize,
    /// Row-major built-in identities.
    pub(crate) members: Vec<BuiltinAggregator>,
}
impl CallHost<'_, '_> {
    /// Project a deferred aggregation role without evaluating it as worksheet data.
    pub(crate) fn aggregators(&mut self, index: usize) -> FunctionResult<Aggregators> {
        let plan = self
            .arguments
            .get(index)
            .ok_or(CellError::Value)?
            .aggregation()
            .map_err(|_| self.budget_exceeded())?
            .ok_or(CellError::Value)?;
        self.resolve_aggregation(&plan, 0)
    }
    fn resolve_aggregation(&mut self, plan: &Plan, depth: usize) -> FunctionResult<Aggregators> {
        if depth >= crate::bytecode::DEPTH || !plan.valid() {
            return Err(CellError::Value.into());
        }
        self.check_values(plan.members.len())?;
        let mut members = Vec::new();
        let mut rows = plan.rows;
        let mut columns = plan.columns;
        for candidate in &plan.members {
            self.charge_work(1)?;
            let projected = self.aggregation_candidate(candidate, depth.saturating_add(1))?;
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
                .ok_or_else(|| self.budget_exceeded())?;
            if count > 100_000 {
                return Err(CellError::Value.into());
            }
            self.check_values_capacity(count)?;
            self.check_values(projected.members.len())?;
            members
                .try_reserve(projected.members.len())
                .map_err(|_| self.budget_exceeded())?;
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
        Ok(Aggregators {
            rows,
            columns,
            members,
        })
    }
    fn aggregation_candidate(
        &mut self,
        candidate: &Candidate,
        depth: usize,
    ) -> FunctionResult<Aggregators> {
        let scope = self
            .session
            .sheet_named(self.sheet, candidate.sheet.as_deref())?
            .ok_or(CellError::Ref)?;
        self.check_text(candidate.name.len())?;
        self.charge_work((self.session.workbook.named_ranges.len() as u64).saturating_mul(2))?;
        let selected = self
            .session
            .workbook
            .try_defined_name(&candidate.name, scope)
            .map_err(identifier_failure)?
            .and_then(|name| {
                self.session
                    .workbook
                    .named_ranges
                    .iter()
                    .position(|n| std::ptr::eq(n, name))
            });
        self.session.read_lazy(|| DependencyRead::Name {
            sheet: scope,
            name: candidate.name.clone(),
            selected,
        });
        if let Some(index) = selected {
            self.check_values(self.session.runtime.names.len().saturating_add(1))?;
            let key = (self.sheet, index);
            self.session
                .runtime
                .names
                .try_reserve(1)
                .map_err(|_| self.budget_exceeded())?;
            // SAFETY: The successful local reservation admits this one name-cycle key.
            if !self.session.runtime.names.insert(key) {
                return Err(CellError::Ref.into());
            }
            let result = (|| {
                let formula = &self
                    .session
                    .workbook
                    .named_ranges
                    .get(index)
                    .ok_or(EvaluationFailure::StalePlan)?
                    .formula;
                self.session.runtime.budget.text(formula.len())?;
                let syntax = poirot_parsing::formula::parse_formula(formula)
                    .map_err(|_| CellError::Value)?;
                let plan = crate::bytecode::aggregation::lower_with_registry(
                    &syntax,
                    self.session.extensions,
                )
                .map_err(|_| self.budget_exceeded())?
                .ok_or(CellError::Value)?;
                self.resolve_aggregation(&plan, depth)
            })();
            self.session.runtime.names.remove(&key);
            return result;
        }
        if candidate.sheet.is_some() {
            return Err(CellError::Name.into());
        }
        let operation = BuiltinAggregator::from_name(&candidate.name).ok_or(CellError::Name)?;
        let mut members = Vec::new();
        members
            .try_reserve_exact(1)
            .map_err(|_| self.budget_exceeded())?;
        // SAFETY: One slot was reserved locally and the vector is still empty.
        members.push(operation);
        Ok(Aggregators {
            rows: 1,
            columns: 1,
            members,
        })
    }
}
