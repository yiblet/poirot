//! Sheet names, worksheet cells and dynamic reference text read for reference-building functions.
use super::call_host::CallHost;
use crate::eval::limits::EvalResult;
use crate::eval::session::{CellEvaluator, NameResolver};
use crate::eval::values::{FunctionResult, Value};
use crate::eval::vm::Vm;
use crate::model::{CellAddress, CellError, CellValue};

/// Resolves sheet names, worksheet cells and dynamic reference syntax for one call.
pub(in crate::eval) struct DynamicReferences<'h, 'call, 'workbook> {
    /// The call whose session resolves and records the reads.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> DynamicReferences<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's reads.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Read a local worksheet cell through dependency, overlay, and spill resolution.
    pub(in crate::eval) fn sheet_cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<CellValue> {
        CellEvaluator::new(self.host.session).cell(sheet, address)
    }

    /// Resolve a captured local sheet name with the evaluator's dependency tracking.
    pub(in crate::eval) fn resolve_sheet(&mut self, name: String) -> EvalResult<Option<usize>> {
        NameResolver::new(self.host.session).sheet(self.host.sheet, &Some(name))
    }

    /// Inspect the prepared formula's parser policy for dynamic reference endpoints.
    pub(in crate::eval) fn parse_policy(&self) -> poirot_parsing::formula::FormulaParsePolicy {
        self.host.session.sources.plan.map_or_else(
            poirot_parsing::formula::FormulaParsePolicy::default,
            |plan| plan.policy().parse,
        )
    }

    /// Count a dynamic parse attempt when run profiling is enabled.
    pub(in crate::eval) fn record_dynamic_parse(&mut self) {
        if self.host.session.state.runtime.budget.profiling {
            self.host
                .session
                .state
                .runtime
                .run_usage
                .dynamic_parse_attempts = self
                .host
                .session
                .state
                .runtime
                .run_usage
                .dynamic_parse_attempts
                .saturating_add(1);
        }
    }

    /// Evaluate validated dynamic reference syntax with the captured extension registry.
    pub(in crate::eval) fn dynamic_reference(
        &mut self,
        sheet: usize,
        syntax: &poirot_parsing::formula::Expr,
    ) -> FunctionResult<Value> {
        let program = crate::eval::compile::bytecode::compile_with_registry(
            syntax,
            self.host.session.sources.extensions,
        )
        .map_err(|_| CellError::Ref)?;
        Ok(Vm::new(self.host.session).run(sheet, self.host.address, &program)?)
    }
}
