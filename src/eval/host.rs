//! Deferred, budgeted Session boundary for independently owned family bridges.
//! This module does not resolve function names or select a family implementation.
use super::*;
use crate::eval::array_operator::Shape;
use crate::eval::budgeted_value::{copy_cell, copy_value};
use crate::eval::selection::Selection;

mod calendar;
mod lookup;
pub(in crate::eval) use lookup::{ClassicLookupKind, LookupTable};
mod aggregation;
pub(in crate::eval) use aggregation::Aggregators;
mod coercionv2;
mod numeric_policy;
mod operand;
mod readers;
mod resolution;
mod services;
mod subtotal;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod traversal;

/// Deferred operand channels, provenance, and coercion policies.
pub(in crate::eval) use operand::{
    HostOperand, MissingPolicy, NumericPolicy, OperandDescription, OperandKind, OperandOrigin,
    ScalarOperand,
};
use resolution::ScalarSelectionPolicy;
#[cfg(test)]
use traversal::TraversalPolicy;

/// Argument roles are fixed by the consumer before inspecting or projecting a value.
#[derive(Clone, Copy)]
enum ArgumentDemand {
    Scalar,
    Inherited(expression_execution::ExpressionDemand),
    ArrayPrefix(usize),
    ArraySlots(&'static [usize]),
}
impl ArgumentDemand {
    fn at(self, index: usize) -> expression_execution::ExpressionDemand {
        use expression_execution::ExpressionDemand;
        match self {
            Self::Inherited(demand) => demand,
            Self::ArrayPrefix(count) if index < count => ExpressionDemand::Array,
            Self::ArraySlots(slots) if slots.contains(&index) => ExpressionDemand::Array,
            _ => ExpressionDemand::Scalar,
        }
    }
}

/// Budgeted bridge from function adapters to a running evaluator and its deferred arguments.
/// Arguments are evaluated on demand and retained in VM operand slots, allowing
/// several projections without repeating their expression effects.
pub(in crate::eval) struct CallHost<'call, 'workbook> {
    session: &'call mut Session<'workbook>,
    sheet: usize,
    address: CellAddress,
    arguments: ArgumentsRef<'call>,
    argument_base: usize,
    argument_slots: usize,
    prepared_arguments: usize,
    argument_demand: ArgumentDemand,
}
/// Presents one deferred argument through the function-core scalar input interface.
/// Forcing it uses the host's memoized evaluation and preserves the chosen omission policy.
struct ScalarInput<'input, 'call, 'workbook> {
    host: &'input mut CallHost<'call, 'workbook>,
    index: usize,
    missing: MissingPolicy,
}
/// Reads selected array members while the evaluator builds a rectangular result.
/// The restricted session access lets branch selection avoid reading unchosen coordinates.
pub(in crate::eval) struct SelectedReader<'read, 'workbook> {
    session: &'read mut Session<'workbook>,
}
impl SelectedReader<'_, '_> {
    /// Read one selected branch member without evaluating unchosen coordinates.
    pub(in crate::eval) fn member(
        &mut self,
        value: &Value,
        shape: Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        self.session.rectangular_element(value, shape, row, column)
    }
}
impl poirot_functions::scalar::Input for ScalarInput<'_, '_, '_> {
    /// Selected scalar operand retaining its argument provenance.
    type Value = ScalarOperand;
    /// Family failure channel preserving spreadsheet versus operational errors.
    type Error = FunctionFailure;
    fn force(&mut self) -> FunctionResult<ScalarOperand> {
        self.host.scalar_data(self.index, self.missing)
    }
}

impl<'call, 'workbook> CallHost<'call, 'workbook> {
    /// Resolve selected rectangular branches through the evaluator's checked selection path.
    pub(in crate::eval) fn selected_array_results(
        &mut self,
        selected: Vec<Selection>,
        output: Shape,
        result_ordinals: impl Iterator<Item = usize>,
    ) -> EvalResult<Value> {
        self.session.selected_array_results(
            self.sheet,
            self.address,
            self.arguments,
            selected,
            output,
            result_ordinals,
        )
    }
    /// Evaluate one scalar argument with the caller's pinned scalar observation policy.
    pub(in crate::eval) fn scalar_argument(&mut self, index: usize) -> EvalResult<CellValue> {
        self.session
            .argument(self.sheet, self.address, self.arguments, index)
    }
    /// Transfer one selected argument with its observation identity and original value channel.
    /// Callers retain the returned value and invoke this once per selected ordinal.
    pub(in crate::eval) fn selected_argument(
        &mut self,
        index: usize,
    ) -> EvalResult<(Option<execution_observation::OperationId>, Value)> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        self.session
            .observed_value_argument(self.sheet, self.address, expression, index)
    }
    /// Inspect whether a selected expression is syntactically omitted without forcing it.
    pub(in crate::eval) fn selected_is_missing(&self, index: usize) -> bool {
        self.arguments
            .get(index)
            .is_some_and(|arg| arg.is_missing())
    }
    /// Read the compatibility policy and record the invocation's dependency on it.
    pub(in crate::eval) fn selection_builtin_policy(&mut self) -> BuiltinSemanticsPolicy {
        self.session.read(DependencyRead::BuiltinPolicy);
        self.session.builtin_policy
    }
    /// Inspect whether a selected value retains array selection semantics.
    pub(in crate::eval) fn retains_selected_array(&self, value: &Value) -> bool {
        self.session.retains_array(value)
    }
    /// Scalarize an already selected value with legacy reference intersection.
    pub(in crate::eval) fn select_legacy_scalar(&mut self, value: Value) -> EvalResult<CellValue> {
        self.session.select_legacy_scalar(self.address, value)
    }
    /// Coerce a selected scalar with the evaluator's exact text and locale charges.
    pub(in crate::eval) fn selected_number(&mut self, value: &CellValue) -> FunctionResult<f64> {
        self.session.coercev2_scalar_number(value)
    }
    /// Inspect rectangular shape without dereferencing selected reference members.
    pub(in crate::eval) fn selected_shape(&self, value: &Value) -> Option<Shape> {
        Shape::inspect(value)
    }
    /// Read one selected rectangular member using the evaluator's broadcast policy.
    pub(in crate::eval) fn selected_member(
        &mut self,
        value: &Value,
        shape: Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        self.session.rectangular_element(value, shape, row, column)
    }
    /// Construct a selected rectangle with existing output admission and lazy member reads.
    pub(in crate::eval) fn selected_array(
        &mut self,
        shape: Shape,
        mut member: impl FnMut(&mut SelectedReader<'_, '_>, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<Value> {
        self.session
            .rectangular_values(self.address, shape, |session, row, column| {
                member(&mut SelectedReader { session }, row, column)
            })
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
            .runtime
            .budget
            .wildcard_step(visited)
            .map_err(Into::into)
    }
    /// Record an argument consumer's dependence on numeric locale defaults.
    pub(in crate::eval) fn read_number_locale(&mut self) {
        self.session.read(DependencyRead::NumberLocale);
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

    /// Create a call-local memo over shared execution state and deferred arguments.
    pub(in crate::eval) fn new(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
    ) -> Self {
        let argument_base = session.runtime.vm.operands.len();
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
        let demand = session.expression_demand;
        let mut host = Self::new(session, sheet, address, arguments);
        host.argument_demand = ArgumentDemand::Inherited(demand);
        host
    }
    /// Evaluate and observe one selector probe, transferring arrays without reevaluation.
    /// Scalar probes retain the host's omission and implicit-intersection policies.
    pub(in crate::eval) fn probe_argument(&mut self, index: usize) -> EvalResult<Value> {
        let expression = self
            .arguments
            .get(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        let observation = if self.session.execution_observer.is_some() {
            Some(self.session.observation_begin(
                EvaluationTarget {
                    sheet: self.sheet,
                    address: self.address,
                },
                execution_observation::OperationKind::Argument {
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
            if self.session.retains_array(
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
            self.session.observation_end(
                operation,
                previous,
                result.as_ref().map(execution_observation::observed_value),
            )?;
        }
        result
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
    /// Return the run's builtin compatibility contract.
    pub(in crate::eval) fn builtin_semantics_policy(&self) -> BuiltinSemanticsPolicy {
        self.session.builtin_policy
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
    /// Consume and record a budgeted draw from the current cell's reproducible stream.
    pub(in crate::eval) fn random_draw(
        &mut self,
        interval: Option<crate::eval::random::Interval>,
    ) -> Result<f64, FunctionFailure> {
        let work = if interval.is_some() {
            crate::eval::random::INTEGER_WORK
        } else {
            1
        };
        self.check_values(1)?;
        if !self
            .session
            .runtime
            .random_streams
            .contains_key(&(self.sheet, self.address))
        {
            self.session
                .runtime
                .budget
                .check_allocation(self.session.runtime.random_streams.len().saturating_add(1))?;
            if self.session.runtime.budget.profiling {
                self.session.runtime.budget.check_allocation(
                    self.session
                        .runtime
                        .budget
                        .usage
                        .random_accesses
                        .len()
                        .saturating_add(1),
                )?;
            }
            // Bounds describe both entire collections; work charges only the two
            // new records. Previous slots are neither copied nor rematerialized.
            let records = 1 + u64::from(self.session.runtime.budget.profiling);
            self.session
                .runtime
                .budget
                .charge(crate::eval::WorkKind::Materialization, records)?;
        }
        self.charge_work(work + 1)?;
        let seed = self.session.context.randomness_seed;
        // Each stream owns a stable trace slot: distinct cells never rescan
        // earlier stream records. Both collections were bounded before insertion.
        let runtime = &mut *self.session.runtime;
        let profiling = runtime.budget.profiling;
        let accesses = &mut runtime.budget.usage.random_accesses;
        let (stream, usage_slot) = runtime
            .random_streams
            .entry((self.sheet, self.address))
            .or_insert_with(|| {
                let slot = accesses.len();
                if profiling {
                    accesses.push(crate::eval::random::RandomAccess {
                        seed,
                        sheet: self.sheet as u64,
                        row: self.address.row,
                        column: self.address.col,
                        version: crate::eval::random::STREAM_VERSION,
                        draws: 0,
                        words: 0,
                    });
                }
                (
                    crate::eval::random::Stream::new(
                        seed,
                        self.sheet as u64,
                        self.address.row,
                        self.address.col,
                    ),
                    slot,
                )
            });
        let (value, words) = match interval {
            None => (Some(stream.unit()), 1),
            Some(interval) => match stream.integer(interval) {
                Some((value, words)) => (Some(value), words),
                None => (None, crate::eval::random::INTEGER_WORK),
            },
        };
        if profiling {
            let access = accesses
                .get_mut(*usage_slot)
                .ok_or(EvaluationFailure::StalePlan)?;
            access.draws = access.draws.saturating_add(1);
            access.words = access.words.saturating_add(words);
        }
        self.session
            .read(DependencyRead::Random(crate::eval::random::RandomAccess {
                seed,
                sheet: self.sheet as u64,
                row: self.address.row,
                column: self.address.col,
                version: crate::eval::random::STREAM_VERSION,
                draws: 1,
                words,
            }));
        value.ok_or_else(|| {
            FunctionFailure::Operational(self.capability_failure(
                CapabilityId::FunctionAdapter,
                "bounded integer entropy rejection exhausted",
            ))
        })
    }
    /// Native regex execution guards selected independently of calculation work.
    pub(in crate::eval) fn regex_settings(&self) -> crate::eval::RegexSettings {
        self.session.regex
    }
    /// Report an allocation refusal without inventing an exhausted quota.
    pub(in crate::eval) fn allocation_failure(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::ResourceFailureKind::Allocation,
            "allocating a function result or scratch buffer",
        )
    }
    /// Report an unrepresentable size independently of caller quotas.
    pub(in crate::eval) fn size_overflow(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::ResourceFailureKind::SizeOverflow,
            "function size or work estimate overflow",
        )
    }
    /// Preserve a native backend's execution guard as an operational failure.
    pub(in crate::eval) fn resource_failure(
        &self,
        kind: crate::eval::ResourceFailureKind,
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
    /// Count syntactic argument positions.
    pub(in crate::eval) fn len(&self) -> usize {
        self.arguments.len()
    }
    #[cfg(test)]
    fn context(&self) -> EvaluationInputs {
        self.session.context
    }
    #[cfg(test)]
    fn date_system(&self) -> crate::model::DateSystem {
        self.session.workbook.date_system
    }
    /// Return current caller coordinates.
    pub(in crate::eval) fn target(&self) -> EvaluationTarget {
        EvaluationTarget {
            sheet: self.sheet,
            address: self.address,
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
    /// Admit collection capacity and charge materialization work.
    pub(in crate::eval) fn check_values(&mut self, elements: usize) -> EvalResult<()> {
        self.session.runtime.budget.allocation(elements)
    }
    /// Admit text storage and charge processing work before copying.
    pub(in crate::eval) fn check_text(&mut self, bytes: usize) -> EvalResult<()> {
        self.session.runtime.budget.text(bytes)
    }
    /// Check growing text capacity when copied bytes are charged separately.
    pub(in crate::eval) fn check_text_capacity(&self, bytes: usize) -> EvalResult<()> {
        self.session.runtime.budget.check_text(bytes)
    }
    /// Check output capacity when materialization work is charged separately.
    pub(in crate::eval) fn check_values_capacity(&self, elements: usize) -> EvalResult<()> {
        self.session.runtime.budget.check_allocation(elements)
    }

    /// Charge helper work against active run and formula limits.
    pub(in crate::eval) fn charge_work(&mut self, units: u64) -> EvalResult<()> {
        self.session
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, units)
    }
    #[cfg(test)]
    fn remaining_work(&self) -> u64 {
        self.session.runtime.budget.remaining_work()
    }

    /// Borrow a previously forced argument from this call's VM stack frame.
    fn operand(&self, index: usize) -> Option<&HostOperand> {
        if index >= self.argument_slots {
            return None;
        }
        self.session
            .runtime
            .vm
            .operands
            .get(self.argument_base.checked_add(index)?)?
            .as_ref()
    }
    fn prepare(&mut self, index: usize) -> FunctionResult<()> {
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
            let stack = &mut self.session.runtime.vm.operands;
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
        let array_missing = expression.array_missing(&mut self.session.runtime.budget)?;
        let value = self.session.expr_with_demand(
            self.sheet,
            self.address,
            expression,
            self.argument_demand.at(index),
        )?;
        let missing = missing || matches!(value, Value::Missing);
        self.session
            .runtime
            .budget
            .check_allocation(self.prepared_arguments.saturating_add(1))?;
        self.session
            .runtime
            .budget
            .charge(WorkKind::Materialization, 1)?;
        let slot = self
            .argument_base
            .checked_add(index)
            .ok_or(EvaluationFailure::StalePlan)?;
        *self
            .session
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
    /// Copy one retained cell value after admitting its owned text storage.
    pub(in crate::eval) fn copy_cell_value(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<CellValue> {
        Ok(copy_cell(&mut self.session.runtime.budget, value)?)
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
        let runtime = &mut *self.session.runtime;
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
    /// Fold an eager aggregate operand with checked batch admission and no member storage.
    pub(in crate::eval) fn fold_aggregate_operand<T>(
        &mut self,
        operand: HostOperand,
        state: T,
        reserve_storage: impl FnMut(&mut T, usize) -> Result<(), std::collections::TryReserveError>,
        consume: impl FnMut(&mut T, ScalarOperand) -> EvalResult<()>,
    ) -> EvalResult<T> {
        self.session
            .fold_items(operand, state, reserve_storage, consume)
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
        self.session.fold_ordered_sheet_span(areas, state, consume)
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
        let runtime = &mut *self.session.runtime;
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
                value: coercev2_missing_scalar(policy)?,
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
                    self.session.signed_read(plan)?.into(),
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
                let plan = resolution::CellReadPlan::implicit(area, self.address)?;
                (
                    self.session.cell(plan.sheet, plan.address)?.into(),
                    OperandOrigin::ReferencedCell,
                )
            }
        };
        Ok(ScalarOperand {
            value: self.session.select_legacy_scalar(self.address, value)?,
            origin,
            missing: false,
        })
    }
    /// Reject a helper's unattributed internal resource exhaustion operationally.
    pub(in crate::eval) fn budget_exceeded(&self) -> EvaluationFailure {
        self.resource_failure(
            crate::eval::ResourceFailureKind::BackendGuard,
            "algorithm resource guard refused execution; native counts are unavailable",
        )
    }
    /// Record formatting consumption and copy explicitly configured currency settings.
    pub(in crate::eval) fn currency_context(
        &mut self,
    ) -> FunctionResult<crate::eval::configuration::CurrencyContext> {
        self.session.read(DependencyRead::Formatting);
        let context = self.session.currency.ok_or_else(|| {
            self.unsupported(
                CapabilityId::Formatting,
                "currency localization is not declared by the captured run configuration",
            )
        })?;
        // Both the clone and renderer are bounded before any owned string allocation.
        let bytes = [
            &context.decimal_separator,
            &context.grouping_separator,
            &context.positive_prefix,
            &context.positive_suffix,
            &context.negative_prefix,
            &context.negative_suffix,
        ]
        .iter()
        .fold(0usize, |total, text| total.saturating_add(text.len()));
        self.check_text(bytes)?;
        self.check_values(6)?;
        self.charge_work(bytes as u64)?;
        Ok(context.clone())
    }
}

/// Explicit missing-argument projection. The caller retains omission metadata;
/// this value is only the requested legacy scalar representation.
fn coercev2_missing_scalar(policy: MissingPolicy) -> FunctionResult<CellValue> {
    use poirot_functions::coercion::compatibility::MissingScalarPolicy;
    let policy = match policy {
        MissingPolicy::Blank => MissingScalarPolicy::Blank,
        #[cfg(test)]
        MissingPolicy::Zero => MissingScalarPolicy::Zero,
        #[cfg(test)]
        MissingPolicy::Reject => MissingScalarPolicy::Reject,
    };
    poirot_functions::coercion::compatibility::coercev2_missing_scalar(policy).map_err(Into::into)
}

impl Drop for CallHost<'_, '_> {
    fn drop(&mut self) {
        self.session
            .runtime
            .vm
            .operands
            .truncate(self.argument_base);
    }
}
