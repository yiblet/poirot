//! Metered sheet and scoped-name resolution with dependency reporting.

use super::read_recorder::ReadRecorder;
use crate::eval::limits::{EvalResult, EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::cells::FormulaExecution;
use crate::eval::session::composition::Session;
use crate::eval::values::{Value, err, identifier_failure};
use crate::model::CellAddress;
use crate::model::CellError::{Name, Ref};

/// Resolves sheet qualifiers and defined names, reporting the selection as a dependency.
pub(in crate::eval) struct NameResolver<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> NameResolver<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Resolve an owned optional sheet qualifier using checked workbook lookup.
    pub(in crate::eval) fn sheet(
        &mut self,
        current: usize,
        name: &Option<String>,
    ) -> EvalResult<Option<usize>> {
        self.sheet_named(current, name.as_deref())
    }

    /// Resolve a borrowed qualifier and record the selected sheet or lookup miss.
    pub(in crate::eval) fn sheet_named(
        &mut self,
        current: usize,
        name: Option<&str>,
    ) -> EvalResult<Option<usize>> {
        let Some(name) = name else {
            return Ok(Some(current));
        };
        let mut diagnostic = None;
        for (index, sheet) in self.session.sources.workbook.sheets().iter().enumerate() {
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::NamespaceScan, 1)?;
            match crate::model::identifiers::checked_matches(sheet.name(), name) {
                Ok(false) => continue,
                Err(error) => {
                    diagnostic = Some(error);
                    continue;
                }
                Ok(true) => {}
            }
            ReadRecorder::new(self.session).read_lazy(|| DependencyRead::Sheet {
                current,
                name: name.to_owned(),
                selected: Some(index),
            });
            return Ok(Some(index));
        }
        ReadRecorder::new(self.session).read_lazy(|| DependencyRead::Sheet {
            current,
            name: name.to_owned(),
            selected: None,
        });
        diagnostic.map_or(Ok(None), |error| Err(identifier_failure(error)))
    }

    /// Name selection uses explicit scope; token evaluation retains caller context.
    pub(in crate::eval) fn defined_name(
        &mut self,
        caller: usize,
        scope: usize,
        at: CellAddress,
        name: &str,
    ) -> EvalResult<Value> {
        self.session.state.runtime.budget.text(name.len())?;
        self.session.state.runtime.budget.charge(
            WorkKind::NamespaceScan,
            (self.session.sources.workbook.named_ranges.len() as u64).saturating_mul(2),
        )?;
        let selected = self
            .session
            .sources
            .workbook
            .try_defined_name(name, scope)
            .map_err(identifier_failure)?
            .and_then(|selected| {
                self.session
                    .sources
                    .workbook
                    .named_ranges
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, selected))
            });
        ReadRecorder::new(self.session).read_lazy(|| DependencyRead::Name {
            sheet: scope,
            name: name.to_owned(),
            selected,
        });
        let Some(index) = selected else {
            return Ok(err(Name));
        };
        // Different qualifiers can select one global NAME record. Cycle identity
        // follows that record and the caller context, rather than its spelling.
        let key = (caller, index);
        if !self.session.state.runtime.stack.names.insert(key) {
            return Ok(err(Ref));
        }
        let result = (|| {
            let formula = &self
                .session
                .sources
                .workbook
                .named_ranges
                .get(index)
                .ok_or(EvaluationError::StalePlan)?
                .formula;
            self.session.state.runtime.budget.text(formula.len())?;
            if self.session.state.runtime.budget.profiling {
                self.session
                    .state
                    .runtime
                    .run_usage
                    .named_formula_executions = self
                    .session
                    .state
                    .runtime
                    .run_usage
                    .named_formula_executions
                    .saturating_add(1);
            }
            let prepared = match self
                .session
                .sources
                .plan
                .and_then(|plan| plan.defined_name(index))
            {
                Some(prepared) => std::sync::Arc::clone(prepared),
                None => self.session.sources.prepare_source(formula),
            };
            FormulaExecution::new(self.session).prepared_expression(caller, at, &prepared)
        })();
        self.session.state.runtime.stack.names.remove(&key);
        result
    }
}
