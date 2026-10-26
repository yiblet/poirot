//! Metered sheet and scoped-name resolution with dependency reporting.
use super::*;

impl<'a> Session<'a> {
    /// Resolve an owned optional sheet qualifier using checked workbook lookup.
    pub(super) fn sheet(
        &mut self,
        current: usize,
        name: &Option<String>,
    ) -> EvalResult<Option<usize>> {
        self.sheet_named(current, name.as_deref())
    }
    /// Resolve a borrowed qualifier and record the selected sheet or lookup miss.
    pub(super) fn sheet_named(
        &mut self,
        current: usize,
        name: Option<&str>,
    ) -> EvalResult<Option<usize>> {
        let Some(name) = name else {
            return Ok(Some(current));
        };
        let mut diagnostic = None;
        for (index, sheet) in self.workbook.sheets().iter().enumerate() {
            self.runtime.budget.charge(WorkKind::NamespaceScan, 1)?;
            match crate::model::identifiers::checked_matches(sheet.name(), name) {
                Ok(false) => continue,
                Err(error) => {
                    diagnostic = Some(error);
                    continue;
                }
                Ok(true) => {}
            }
            self.read_lazy(|| DependencyRead::Sheet {
                current,
                name: name.to_owned(),
                selected: Some(index),
            });
            return Ok(Some(index));
        }
        self.read_lazy(|| DependencyRead::Sheet {
            current,
            name: name.to_owned(),
            selected: None,
        });
        diagnostic.map_or(Ok(None), |error| Err(identifier_failure(error)))
    }
    /// Name selection uses explicit scope; token evaluation retains caller context.
    pub(super) fn defined_name(
        &mut self,
        caller: usize,
        scope: usize,
        at: CellAddress,
        name: &str,
    ) -> EvalResult<Value> {
        self.runtime.budget.text(name.len())?;
        self.runtime.budget.charge(
            WorkKind::NamespaceScan,
            (self.workbook.named_ranges.len() as u64).saturating_mul(2),
        )?;
        let selected = self
            .workbook
            .try_defined_name(name, scope)
            .map_err(identifier_failure)?
            .and_then(|selected| {
                self.workbook
                    .named_ranges
                    .iter()
                    .position(|candidate| std::ptr::eq(candidate, selected))
            });
        self.read_lazy(|| DependencyRead::Name {
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
        if !self.runtime.names.insert(key) {
            return Ok(err(Ref));
        }
        let result = (|| {
            let formula = &self
                .workbook
                .named_ranges
                .get(index)
                .ok_or(EvaluationFailure::StalePlan)?
                .formula;
            self.runtime.budget.text(formula.len())?;
            if self.runtime.budget.profiling {
                self.runtime.run_usage.named_formula_executions = self
                    .runtime
                    .run_usage
                    .named_formula_executions
                    .saturating_add(1);
            }
            let prepared = match self.plan.and_then(|plan| plan.defined_name(index)) {
                Some(prepared) => std::sync::Arc::clone(prepared),
                None => self.prepare_source(formula),
            };
            self.prepared_expression(caller, at, &prepared)
        })();
        self.runtime.names.remove(&key);
        result
    }
}
