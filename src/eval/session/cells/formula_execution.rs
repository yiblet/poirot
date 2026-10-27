//! Prepared and bound formula execution at a target cell.

use crate::eval::compile;
use crate::eval::compile::compiler::PreparedFormula;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::session::composition::Session;
use crate::eval::session::projection::ResultProjection;
use crate::eval::values::{VALUE_ERROR, Value, err};
use crate::eval::vm::Vm;
use crate::model::CellError::Ref;
use crate::model::CellValue::Error;
use crate::model::{CellAddress, CellValue};

/// Executes prepared or bound formulas within cell-attributed budget frames.
pub(in crate::eval) struct FormulaExecution<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> FormulaExecution<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Execute prepared source, preserving deferred parse failures and registry checks.
    pub(in crate::eval) fn prepared_expression(
        &mut self,
        sheet: usize,
        at: CellAddress,
        prepared: &PreparedFormula,
    ) -> EvalResult<Value> {
        if self
            .session
            .sources
            .plan
            .is_some_and(|plan| plan.policy() != prepared.policy())
        {
            return Err(EvaluationError::StalePlan);
        }
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && prepared.spill_shape().is_some()
            && let Some(reads) = &mut self.session.reuse.overlay_reads
        {
            reads.mark_spill_sensitive((sheet, at));
        }
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.compiled_lookups = self
                .session
                .state
                .runtime
                .run_usage
                .compiled_lookups
                .saturating_add(1);
        }
        match prepared {
            PreparedFormula::Ready(formula) => {
                Vm::new(self.session).run(sheet, at, formula.bytecode())
            }
            PreparedFormula::CompilationFailure { error, .. } => {
                let source = Some(EvaluationTarget { sheet, address: at });
                let capability = CapabilityId::FormulaCompilation;
                let reason = match error {
                    crate::eval::compile::bytecode::CompileError::ResourceLimit => {
                        crate::eval::limits::EvaluationReasonError::CompilationResourceLimit
                    }
                    crate::eval::compile::bytecode::CompileError::InvalidArray => {
                        crate::eval::limits::EvaluationReasonError::CompilationInvalidArray
                    }
                    crate::eval::compile::bytecode::CompileError::InvalidReference => {
                        crate::eval::limits::EvaluationReasonError::CompilationInvalidReference
                    }
                };
                Err(
                    if matches!(
                        error,
                        crate::eval::compile::bytecode::CompileError::ResourceLimit
                    ) {
                        EvaluationError::ResourceLimit {
                            capability,
                            target: source,
                            reason,
                        }
                    } else {
                        EvaluationError::UnsupportedCapability {
                            capability,
                            target: source,
                            reason,
                        }
                    },
                )
            }
            PreparedFormula::ParseFailure { error, .. } => {
                let source = Some(EvaluationTarget { sheet, address: at });
                let capability = CapabilityId::FormulaParsing;
                let reason = crate::eval::limits::EvaluationReasonError::Parse(
                    std::sync::Arc::new(error.clone()),
                );
                match error.kind() {
                    poirot_parsing::formula::ParseErrorKind::Syntax => Ok(err(VALUE_ERROR)),
                    poirot_parsing::formula::ParseErrorKind::ResourceLimit => {
                        Err(EvaluationError::ResourceLimit {
                            capability,
                            target: source,
                            reason,
                        })
                    }
                    poirot_parsing::formula::ParseErrorKind::InvalidPolicy => {
                        Err(EvaluationError::UnsupportedCapability {
                            capability,
                            target: source,
                            reason,
                        })
                    }
                }
            }
            PreparedFormula::RegistryMismatch { .. } => Err(EvaluationError::StalePlan),
        }
    }

    /// Execute a supplied prepared formula within a cell-attributed budget frame.
    pub(in crate::eval) fn prepared_at(
        &mut self,
        target: EvaluationTarget,
        prepared: &PreparedFormula,
    ) -> EvalResult<CellValue> {
        if self.session.sources.workbook.sheet(target.sheet).is_none()
            || target.address.validate().is_err()
        {
            return Ok(Error(Ref));
        }
        let demand = self.session.policies.root_demand();
        let previous_demand = std::mem::replace(&mut self.session.state.expression_demand, demand);
        let previous = self.session.state.runtime.stack.consumer.replace(target);
        self.session.state.runtime.budget.push_cell(target);
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.formula_executions = self
                .session
                .state
                .runtime
                .run_usage
                .formula_executions
                .saturating_add(1);
        }
        let result = (|| {
            self.session
                .state
                .runtime
                .budget
                .text(prepared.source().len())?;
            let value = self.prepared_expression(target.sheet, target.address, prepared)?;
            let value = ResultProjection::new(self.session).finish_expression(
                target.sheet,
                target.address,
                value,
            )?;
            Ok(value)
        })();
        self.session.state.runtime.budget.pop_cell();
        self.session.state.runtime.stack.consumer = previous;
        self.session.state.expression_demand = previous_demand;

        result
    }

    /// Execute a supplied bytecode plan after checking its extension identity and target.
    pub(in crate::eval) fn bound_at(
        &mut self,
        target: EvaluationTarget,
        formula: &compile::bound::FormulaPlan,
    ) -> EvalResult<CellValue> {
        if formula.extension_identity().is_some_and(|identity| {
            self.session
                .sources
                .extensions
                .is_none_or(|registry| identity != &registry.identity())
        }) {
            return Err(EvaluationError::StalePlan);
        }
        if self.session.sources.workbook.sheet(target.sheet).is_none()
            || target.address.validate().is_err()
        {
            return Ok(Error(Ref));
        }
        let demand = self.session.policies.root_demand();
        let previous_demand = std::mem::replace(&mut self.session.state.expression_demand, demand);
        let previous = self.session.state.runtime.stack.consumer.replace(target);
        self.session.state.runtime.budget.push_cell(target);
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.formula_executions = self
                .session
                .state
                .runtime
                .run_usage
                .formula_executions
                .saturating_add(1);
        }
        let result = Vm::new(self.session)
            .run(target.sheet, target.address, formula.bytecode())
            .and_then(|v| {
                ResultProjection::new(self.session).finish_expression(
                    target.sheet,
                    target.address,
                    v,
                )
            });
        self.session.state.runtime.budget.pop_cell();
        self.session.state.runtime.stack.consumer = previous;
        self.session.state.expression_demand = previous_demand;
        result
    }
}
