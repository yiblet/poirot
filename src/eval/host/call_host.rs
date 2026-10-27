use super::argument_demand::ArgumentDemand;
use super::cell_read_plan::CellReadPlan;
use super::host_operand::HostOperand;
use super::missing_policy::MissingPolicy;
use super::missing_scalar::coerce_missing_scalar;
use super::operand_kind::OperandKind;
use super::scalar_input::ScalarInput;
use super::scalar_operand::ScalarOperand;
use super::scalar_selection_policy::ScalarSelectionPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::observe;
use crate::eval::session::ObservationRecorder;
use crate::eval::session::Session;
use crate::eval::session::{ArgumentReader, CellEvaluator, ItemFolder, ResultProjection};
use crate::eval::values::core_projection::semantic;
use crate::eval::values::{FunctionFailure, FunctionResult, VALUE_ERROR, Value, err};
use crate::eval::values::{OperandOrigin, copy_cell, copy_value};
use crate::eval::vm::ArgumentsRef;
use crate::eval::vm::ExpressionRunner;
use crate::model::{CellAddress, CellValue};

/// Budgeted bridge from function adapters to a running evaluator and its deferred arguments.
/// Arguments are evaluated on demand and retained in VM operand slots, allowing
/// several projections without repeating their expression effects.
pub(in crate::eval) struct CallHost<'call, 'workbook> {
    pub(super) session: &'call mut Session<'workbook>,
    pub(super) sheet: usize,
    pub(super) address: CellAddress,
    pub(super) arguments: ArgumentsRef<'call>,
    pub(super) argument_base: usize,
    pub(super) argument_slots: usize,
    pub(super) prepared_arguments: usize,
    pub(super) argument_demand: ArgumentDemand,
}

impl<'call, 'workbook> CallHost<'call, 'workbook> {
    /// Create a call-local memo over shared execution state and deferred arguments.
    pub(in crate::eval) fn new(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
    ) -> Self {
        let argument_base = session.state.runtime.vm.operands.len();
        Self {
            session,
            sheet,
            address,
            arguments: arguments.into(),
            argument_base,
            argument_slots: 0,
            prepared_arguments: 0,
            argument_demand: ArgumentDemand::Scalar,
        }
    }

    /// Type-preserving consumers inherit the caller's expression demand.
    pub(in crate::eval) fn new_inheriting(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
    ) -> Self {
        let demand = session.state.expression_demand;
        let mut host = Self::new(session, sheet, address, arguments);
        host.argument_demand = ArgumentDemand::Inherited(demand);
        host
    }

    /// Matrix consumers request array expression execution before any operand inspection.
    pub(in crate::eval) fn new_array(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
        array_count: usize,
    ) -> Self {
        let mut host = Self::new(session, sheet, address, arguments);
        host.argument_demand = ArgumentDemand::ArrayPrefix(array_count);
        host
    }

    /// Declare noncontiguous array-valued argument roles before forcing any operand.
    pub(in crate::eval) fn new_array_slots(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
        slots: &'static [usize],
    ) -> Self {
        let mut host = Self::new(session, sheet, address, arguments);
        host.argument_demand = ArgumentDemand::ArraySlots(slots);
        host
    }

    /// Count syntactic argument positions.
    pub(in crate::eval) fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Return current caller coordinates.
    pub(in crate::eval) fn target(&self) -> EvaluationTarget {
        EvaluationTarget {
            sheet: self.sheet,
            address: self.address,
        }
    }

    /// Borrow a previously forced argument from this call's VM stack frame.
    pub(super) fn operand(&self, index: usize) -> Option<&HostOperand> {
        if index >= self.argument_slots {
            return None;
        }
        self.session
            .state
            .runtime
            .vm
            .operands
            .get(self.argument_base.checked_add(index)?)?
            .as_ref()
    }

    pub(super) fn prepare(&mut self, index: usize) -> FunctionResult<()> {
        if self.operand(index).is_some() {
            return Ok(());
        }
        let expression = self.arguments.get(index).ok_or(VALUE_ERROR)?;
        let count = index.checked_add(1).ok_or(EvaluationFailure::StalePlan)?;
        if count > self.argument_slots {
            let end = self
                .argument_base
                .checked_add(count)
                .ok_or(EvaluationFailure::StalePlan)?;
            let stack = &mut self.session.state.runtime.vm.operands;
            stack
                .try_reserve(count - self.argument_slots)
                .map_err(|_| EvaluationFailure::ResourceLimit {
                    capability: CapabilityId::FunctionAdapter,
                    source: Some(EvaluationTarget {
                        sheet: self.sheet,
                        address: self.address,
                    }),
                    detail: "VM argument stack allocation failed".into(),
                })?;
            stack.resize_with(end, || None);
            self.argument_slots = count;
        }
        let missing = expression.is_missing();
        let array_missing = expression.array_missing(&mut self.session.state.runtime.budget)?;
        let value = ExpressionRunner::new(self.session).expr_with_demand(
            self.sheet,
            self.address,
            expression,
            self.argument_demand.at(index),
        )?;
        let missing = missing || matches!(value, Value::Missing);
        self.session
            .state
            .runtime
            .budget
            .check_allocation(self.prepared_arguments.saturating_add(1))?;
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::Materialization, 1)?;
        let slot = self
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        *self
            .session
            .state
            .runtime
            .vm
            .operands
            .get_mut(slot)
            .ok_or(EvaluationFailure::StalePlan)? = Some(HostOperand {
            value,
            missing,
            array_missing,
        });
        self.prepared_arguments += 1;
        Ok(())
    }

    /// Resolve one deferred argument and inspect its channel without scalarizing it.
    pub(in crate::eval) fn kind(&mut self, index: usize) -> FunctionResult<OperandKind> {
        self.prepare(index)?;
        Ok(self.operand(index).ok_or(VALUE_ERROR)?.kind())
    }

    /// Return a budgeted copy of one resolved operand with omission metadata intact.
    pub(in crate::eval) fn argument(&mut self, index: usize) -> FunctionResult<HostOperand> {
        self.prepare(index)?;
        let slot = self
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.session.state.runtime;
        let operand = &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?;
        runtime
            .budget
            .allocation(operand.array_missing.iter().map(Vec::len).sum())?;
        Ok(HostOperand {
            value: copy_value(&mut runtime.budget, &operand.value)?,
            missing: operand.missing,
            array_missing: operand.array_missing.clone(),
        })
    }

    /// Evaluate one scalar argument with the caller's pinned scalar observation policy.
    pub(in crate::eval) fn scalar_argument(&mut self, index: usize) -> EvalResult<CellValue> {
        ArgumentReader::new(self.session).argument(self.sheet, self.address, self.arguments, index)
    }

    /// Scalarize an already selected value with legacy reference intersection.
    pub(in crate::eval) fn select_legacy_scalar(&mut self, value: Value) -> EvalResult<CellValue> {
        ResultProjection::new(self.session).select_legacy_scalar(self.address, value)
    }

    /// Borrow one deferred scalar input under an explicit omission policy.
    pub(in crate::eval) fn input(
        &mut self,
        index: usize,
        missing: MissingPolicy,
    ) -> impl poirot_functions::scalar::Input<Value = ScalarOperand, Error = FunctionFailure> + '_
    {
        ScalarInput {
            host: self,
            index,
            missing,
        }
    }

    /// Evaluate and observe one selector probe, transferring arrays without reevaluation.
    /// Scalar probes retain the host's omission and implicit-intersection policies.
    pub(in crate::eval) fn probe_argument(&mut self, index: usize) -> EvalResult<Value> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let observation = if self.session.ports.execution_observer.is_some() {
            Some(ObservationRecorder::new(self.session).observation_begin(
                EvaluationTarget {
                    sheet: self.sheet,
                    address: self.address,
                },
                observe::OperationKind::Argument {
                    ordinal: index,
                    origin: expression.origin(),
                },
            )?)
        } else {
            None
        };
        let result: EvalResult<Value> = (|| {
            if let Err(error) = semantic(self.prepare(index))? {
                return Ok(err(error));
            }
            if self.session.state.expression_demand.retains_array(
                &self
                    .operand(index)
                    .ok_or(EvaluationFailure::StalePlan)?
                    .value,
            ) {
                let slot = self
                    .argument_base
                    .checked_add(index)
                    .ok_or(EvaluationFailure::StalePlan)?;
                return self
                    .session
                    .state
                    .runtime
                    .vm
                    .operands
                    .get_mut(slot)
                    .and_then(Option::take)
                    .map(HostOperand::into_value)
                    .ok_or(EvaluationFailure::StalePlan);
            }
            match semantic(self.scalar_data(index, MissingPolicy::Blank))? {
                Ok(scalar) => Ok(scalar.value.into()),
                Err(error) => Ok(err(error)),
            }
        })();
        if let Some((operation, previous)) = observation {
            ObservationRecorder::new(self.session).observation_end(
                operation,
                previous,
                result.as_ref().map(observe::observed_value),
            )?;
        }
        result
    }

    /// Evaluate a raw argument with the evaluator's original expression demand.
    pub(in crate::eval) fn raw_argument(&mut self, index: usize) -> EvalResult<Value> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        ExpressionRunner::new(self.session).expr(self.sheet, self.address, expression)
    }

    /// Implicit intersection is performed only on this request. Errors stay data here.
    pub(in crate::eval) fn scalar_data(
        &mut self,
        index: usize,
        policy: MissingPolicy,
    ) -> FunctionResult<ScalarOperand> {
        self.prepare(index)?;
        let slot = self
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let runtime = &mut *self.session.state.runtime;
        let operand = &runtime
            .vm
            .operands
            .get(slot)
            .and_then(Option::as_ref)
            .ok_or(VALUE_ERROR)?;
        let selection = ScalarSelectionPolicy::PoiOperandResolver;
        let (row, column) = match &operand.value {
            Value::Array(view) => {
                selection
                    .array_position(view, self.address)
                    .map_err(|detail| EvaluationFailure::UnsupportedCapability {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget {
                            sheet: self.sheet,
                            address: self.address,
                        }),
                        detail: detail.into(),
                    })??
            }
            _ => (0, 0),
        };
        let missing = operand.missing
            || operand
                .array_missing
                .get(row)
                .and_then(|flags| flags.get(column))
                .copied()
                .unwrap_or(false);
        if missing {
            return Ok(ScalarOperand {
                value: coerce_missing_scalar(policy)?,
                origin: if matches!(operand.value, Value::Array(_)) {
                    OperandOrigin::ArrayElement
                } else {
                    OperandOrigin::Direct
                },
                missing: true,
            });
        }
        let (value, origin) = match &operand.value {
            Value::Missing => (Value::Missing, OperandOrigin::Direct),
            Value::Scalar(value) => (
                copy_cell(&mut runtime.budget, value)?.into(),
                OperandOrigin::Direct,
            ),
            Value::SelectedScalar(value, origin) => {
                (copy_cell(&mut runtime.budget, value)?.into(), *origin)
            }
            Value::Array(rows) => (
                copy_cell(
                    &mut runtime.budget,
                    rows.get(row)
                        .and_then(|r| r.get(column))
                        .ok_or(VALUE_ERROR)?,
                )?
                .into(),
                OperandOrigin::ArrayElement,
            ),
            Value::SignedRefs(areas) => {
                if areas.len() != 1 {
                    return Err(self.unsupported(
                        CapabilityId::FunctionAdapter,
                        "signed union cannot be coerced to scalar",
                    ));
                }
                // SAFETY: The branch rejects any area count other than one before reading element zero.
                let area = areas[0];
                runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                let plan = area.implicit(self.address).ok_or(VALUE_ERROR)?;
                (
                    CellEvaluator::new(self.session).signed_read(plan)?.into(),
                    OperandOrigin::ReferencedCell,
                )
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                if areas.len() != 1 && !matches!(operand.value, Value::SheetSpan(_)) {
                    return Err(self.unsupported(
                        CapabilityId::FunctionAdapter,
                        "pinned OperandResolver cannot coerce RefListEval to a scalar",
                    ));
                }
                // SAFETY: The branch rejects any area count other than one before reading element zero.
                let area = areas[0];
                runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                let plan = CellReadPlan::implicit(area, self.address)?;
                (
                    CellEvaluator::new(self.session)
                        .cell(plan.sheet, plan.address)?
                        .into(),
                    OperandOrigin::ReferencedCell,
                )
            }
        };
        Ok(ScalarOperand {
            value: ResultProjection::new(self.session).select_legacy_scalar(self.address, value)?,
            origin,
            missing: false,
        })
    }

    /// Copy one retained cell value after admitting its owned text storage.
    pub(in crate::eval) fn copy_cell_value(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<CellValue> {
        Ok(copy_cell(&mut self.session.state.runtime.budget, value)?)
    }

    /// Fold an eager aggregate operand with checked batch admission and no member storage.
    pub(in crate::eval) fn fold_aggregate_operand<T>(
        &mut self,
        operand: HostOperand,
        state: T,
        reserve_storage: impl FnMut(&mut T, usize) -> Result<(), std::collections::TryReserveError>,
        consume: impl FnMut(&mut T, ScalarOperand) -> EvalResult<()>,
    ) -> EvalResult<T> {
        ItemFolder::new(self.session).fold_items(operand, state, reserve_storage, consume)
    }

    /// Visit a sheet span in sheet order, stopping after the first consumer break.
    pub(in crate::eval) fn fold_ordered_sheet_span<T>(
        &mut self,
        operand: HostOperand,
        state: T,
        consume: impl FnMut(&mut T, ScalarOperand) -> EvalResult<std::ops::ControlFlow<()>>,
    ) -> EvalResult<T> {
        let (value, _, _) = operand.into_parts();
        let Value::SheetSpan(areas) = value else {
            return Err(EvaluationFailure::StalePlan);
        };
        ItemFolder::new(self.session).fold_ordered_sheet_span(areas, state, consume)
    }

    /// Evaluate one aggregate argument without retaining a VM operand slot.
    pub(in crate::eval) fn aggregate_argument_once(
        &mut self,
        index: usize,
    ) -> FunctionResult<HostOperand> {
        let expression = self.arguments.get(index).ok_or(VALUE_ERROR)?;
        let missing = expression.is_missing();
        let array_missing = expression.array_missing(&mut self.session.state.runtime.budget)?;
        let value = ExpressionRunner::new(self.session).expr_with_demand(
            self.sheet,
            self.address,
            expression,
            self.argument_demand.at(index),
        )?;
        Ok(HostOperand {
            missing: missing || matches!(value, Value::Missing),
            value,
            array_missing,
        })
    }

    /// Visit omission markers without allocating or forcing the argument again.
    pub(in crate::eval) fn visit_argument_array_missing(
        &self,
        index: usize,
        visit: impl FnMut(usize, bool) -> EvalResult<()>,
    ) -> EvalResult<()> {
        self.arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?
            .visit_array_missing(visit)
    }

    /// Admit wildcard token/character storage; matching steps are charged separately.
    pub(in crate::eval) fn admit_wildcard(
        &mut self,
        pattern: &str,
        input: &str,
    ) -> FunctionResult<()> {
        if poirot_functions::criteria::has_wildcards(pattern) {
            self.check_values_capacity(
                pattern
                    .chars()
                    .count()
                    .saturating_add(input.chars().count()),
            )?;
        }
        Ok(())
    }

    /// Charge one visited matching state against this match's local limit.
    pub(in crate::eval) fn wildcard_step(&mut self, visited: &mut u64) -> FunctionResult<()> {
        self.session
            .state
            .runtime
            .budget
            .wildcard_step(visited)
            .map_err(Into::into)
    }

    /// Admit collection capacity and charge materialization work.
    pub(in crate::eval) fn check_values(&mut self, elements: usize) -> EvalResult<()> {
        self.session.state.runtime.budget.allocation(elements)
    }

    /// Admit text storage and charge processing work before copying.
    pub(in crate::eval) fn check_text(&mut self, bytes: usize) -> EvalResult<()> {
        self.session.state.runtime.budget.text(bytes)
    }

    /// Check growing text capacity when copied bytes are charged separately.
    pub(in crate::eval) fn check_text_capacity(&self, bytes: usize) -> EvalResult<()> {
        self.session.state.runtime.budget.check_text(bytes)
    }

    /// Check output capacity when materialization work is charged separately.
    pub(in crate::eval) fn check_values_capacity(&self, elements: usize) -> EvalResult<()> {
        self.session.state.runtime.budget.check_allocation(elements)
    }

    /// Charge helper work against active run and formula limits.
    pub(in crate::eval) fn charge_work(&mut self, units: u64) -> EvalResult<()> {
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, units)
    }

    /// Charge evaluator work under its original accounting kind.
    pub(in crate::eval) fn charge_kind(&mut self, kind: WorkKind, units: u64) -> EvalResult<()> {
        self.session.state.runtime.budget.charge(kind, units)
    }

    #[cfg(test)]
    pub(super) fn remaining_work(&self) -> u64 {
        self.session.state.runtime.budget.remaining_work()
    }

    /// Attribute a pinned source exception to the current call's cell.
    pub(in crate::eval) fn known_source_exception(
        &self,
        exception: crate::model::KnownSourceException,
    ) -> EvaluationFailure {
        EvaluationFailure::KnownSourceException {
            source: Some(EvaluationTarget {
                sheet: self.sheet,
                address: self.address,
            }),
            exception,
        }
    }

    /// Report an allocation refusal without inventing an exhausted quota.
    pub(in crate::eval) fn allocation_failure(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::limits::ResourceFailureKind::Allocation,
            "allocating a function result or scratch buffer",
        )
    }

    /// Report an unrepresentable size independently of caller quotas.
    pub(in crate::eval) fn size_overflow(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::limits::ResourceFailureKind::SizeOverflow,
            "function size or work estimate overflow",
        )
    }

    /// Preserve a native backend's execution guard as an operational failure.
    pub(in crate::eval) fn resource_failure(
        &self,
        kind: crate::eval::limits::ResourceFailureKind,
        detail: impl Into<String>,
    ) -> EvaluationFailure {
        EvaluationFailure::Resource {
            kind,
            capability: CapabilityId::FunctionAdapter,
            source: Some(EvaluationTarget {
                sheet: self.sheet,
                address: self.address,
            }),
            detail: detail.into(),
        }
    }

    /// Construct an attributed operational capability rejection.
    pub(in crate::eval) fn capability_failure(
        &self,
        capability: CapabilityId,
        detail: impl Into<String>,
    ) -> EvaluationFailure {
        EvaluationFailure::UnsupportedCapability {
            capability,
            source: Some(self.target()),
            detail: detail.into(),
        }
    }

    /// Wrap a capability rejection in the shared function failure carrier.
    pub(in crate::eval) fn unsupported(
        &self,
        capability: CapabilityId,
        detail: impl Into<String>,
    ) -> FunctionFailure {
        self.capability_failure(capability, detail).into()
    }

    /// Reject a helper's unattributed internal resource exhaustion operationally.
    pub(in crate::eval) fn budget_exceeded(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::limits::ResourceFailureKind::BackendGuard,
            "algorithm resource guard refused execution; native counts are unavailable",
        )
    }
}

impl Drop for CallHost<'_, '_> {
    fn drop(&mut self) {
        self.session
            .state
            .runtime
            .vm
            .operands
            .truncate(self.argument_base);
    }
}
