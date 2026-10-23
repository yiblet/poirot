//! Deferred, budgeted Session boundary for independently owned family bridges.
//! This module does not resolve function names or select a family implementation.
use super::*;

mod coercionv2;
mod numeric_policy;
mod operand;
mod readers;
mod resolution;
mod subtotal;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod traversal;

/// Deferred operand channels, provenance, and coercion policies.
pub(crate) use operand::{
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
    SelectedResults {
        first: usize,
        default: Option<usize>,
        inherited: expression_execution::ExpressionDemand,
    },
}
impl ArgumentDemand {
    fn at(self, index: usize) -> expression_execution::ExpressionDemand {
        use expression_execution::ExpressionDemand;
        match self {
            Self::Inherited(demand) => demand,
            Self::ArrayPrefix(count) if index < count => ExpressionDemand::Array,
            Self::SelectedResults {
                first,
                default,
                inherited,
            } if default == Some(index)
                || index
                    .checked_sub(first)
                    .is_some_and(|offset| offset.is_multiple_of(2)) =>
            {
                inherited
            }
            _ => ExpressionDemand::Scalar,
        }
    }
}

/// Deferred arguments use indexed slots on the persistent VM operand stack.
pub(crate) struct CallHost<'call, 'workbook> {
    session: &'call mut Session<'workbook>,
    sheet: usize,
    address: CellAddress,
    arguments: ArgumentsRef<'call>,
    argument_base: usize,
    argument_slots: usize,
    prepared_arguments: usize,
    argument_demand: ArgumentDemand,
}
/// One deferred operand. The host memoizes argument evaluation across projections.
struct ScalarInput<'input, 'call, 'workbook> {
    host: &'input mut CallHost<'call, 'workbook>,
    index: usize,
    missing: MissingPolicy,
}
impl poirot_functions_core::Input for ScalarInput<'_, '_, '_> {
    /// Selected scalar operand retaining its argument provenance.
    type Value = ScalarOperand;
    /// Family failure channel preserving spreadsheet versus operational errors.
    type Error = FunctionFailure;
    fn force(&mut self) -> FunctionResult<ScalarOperand> {
        self.host.scalar_data(self.index, self.missing)
    }
}

impl<'call, 'workbook> CallHost<'call, 'workbook> {
    /// Admit wildcard token/character storage; matching steps are charged separately.
    pub(crate) fn admit_wildcard(&mut self, pattern: &str, input: &str) -> FunctionResult<()> {
        if poirot_functions_criteria::has_wildcards(pattern) {
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
    pub(crate) fn wildcard_step(&mut self, visited: &mut u64) -> FunctionResult<()> {
        self.session
            .runtime
            .budget
            .wildcard_step(visited)
            .map_err(Into::into)
    }
    /// Record an argument consumer's dependence on numeric locale defaults.
    pub(crate) fn read_number_locale(&mut self) {
        self.session.read(DependencyRead::NumberLocale);
    }
    /// Borrow one deferred scalar input under an explicit omission policy.
    pub(crate) fn input(
        &mut self,
        index: usize,
        missing: MissingPolicy,
    ) -> impl poirot_functions_core::Input<Value = ScalarOperand, Error = FunctionFailure> + '_
    {
        ScalarInput {
            host: self,
            index,
            missing,
        }
    }

    /// Create a call-local memo over shared execution state and deferred arguments.
    pub(crate) fn new(
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
    pub(crate) fn new_inheriting(
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
    /// Inspect whether an evaluated argument retains its rectangular value channel.
    pub(crate) fn retains_array(&mut self, index: usize) -> FunctionResult<bool> {
        self.prepare(index)?;
        Ok(self
            .session
            .retains_array(&self.operand(index).ok_or(VALUE_ERROR)?.value))
    }
    /// Matrix consumers request array expression execution before any operand inspection.
    pub(crate) fn new_array(
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
    /// Selection results inherit the caller's demand; tests and keys remain scalar.
    pub(crate) fn new_selecting(
        session: &'call mut Session<'workbook>,
        sheet: usize,
        address: CellAddress,
        arguments: impl Into<ArgumentsRef<'call>>,
        first: usize,
        default: Option<usize>,
    ) -> Self {
        let inherited = session.expression_demand;
        let mut host = Self::new(session, sheet, address, arguments);
        host.argument_demand = ArgumentDemand::SelectedResults {
            first,
            default,
            inherited,
        };
        host
    }
    /// Return the run's builtin compatibility contract.
    pub(crate) fn builtin_semantics_policy(&self) -> BuiltinSemanticsPolicy {
        self.session.builtin_policy
    }
    /// Attribute a pinned source exception to the current call's cell.
    pub(crate) fn known_source_exception(
        &self,
        exception: poirot_model::KnownSourceException,
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
    pub(crate) fn random_draw(
        &mut self,
        interval: Option<crate::random::Interval>,
    ) -> Result<f64, FunctionFailure> {
        let work = if interval.is_some() {
            crate::random::INTEGER_WORK
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
            self.session.runtime.budget.check_allocation(
                self.session
                    .runtime
                    .budget
                    .usage
                    .random_accesses
                    .len()
                    .saturating_add(1),
            )?;
            // Bounds describe both entire collections; work charges only the two
            // new records. Previous slots are neither copied nor rematerialized.
            self.session
                .runtime
                .budget
                .charge(crate::WorkKind::Materialization, 2)?;
        }
        self.charge_work(work + 1)?;
        let seed = self.session.context.randomness_seed;
        // Each stream owns a stable trace slot: distinct cells never rescan
        // earlier stream records. Both collections were bounded before insertion.
        let runtime = &mut *self.session.runtime;
        let accesses = &mut runtime.budget.usage.random_accesses;
        let (stream, usage_slot) = runtime
            .random_streams
            .entry((self.sheet, self.address))
            .or_insert_with(|| {
                let slot = accesses.len();
                accesses.push(crate::random::RandomAccess {
                    seed,
                    sheet: self.sheet as u64,
                    row: self.address.row,
                    column: self.address.col,
                    version: crate::random::STREAM_VERSION,
                    draws: 0,
                    words: 0,
                });
                (
                    crate::random::Stream::new(
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
                None => (None, crate::random::INTEGER_WORK),
            },
        };
        let access = accesses
            .get_mut(*usage_slot)
            .ok_or(EvaluationFailure::StalePlan)?;
        access.draws += 1;
        access.words += words;
        self.session
            .read(DependencyRead::Random(crate::random::RandomAccess {
                seed,
                sheet: self.sheet as u64,
                row: self.address.row,
                column: self.address.col,
                version: crate::random::STREAM_VERSION,
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
    /// Count syntactic argument positions.
    pub(crate) fn len(&self) -> usize {
        self.arguments.len()
    }
    #[cfg(test)]
    fn context(&self) -> EvaluationInputs {
        self.session.context
    }
    #[cfg(test)]
    fn date_system(&self) -> poirot_model::DateSystem {
        self.session.workbook.date_system
    }
    /// Return current caller coordinates.
    pub(crate) fn target(&self) -> EvaluationTarget {
        EvaluationTarget {
            sheet: self.sheet,
            address: self.address,
        }
    }
    /// Construct an attributed operational capability rejection.
    pub(crate) fn capability_failure(
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
    pub(crate) fn unsupported(
        &self,
        capability: CapabilityId,
        detail: impl Into<String>,
    ) -> FunctionFailure {
        self.capability_failure(capability, detail).into()
    }
    /// Admit collection capacity and charge materialization work.
    pub(crate) fn check_values(&mut self, elements: usize) -> FunctionResult<()> {
        Ok(self.session.runtime.budget.allocation(elements)?)
    }
    /// Admit text storage and charge processing work before copying.
    pub(crate) fn check_text(&mut self, bytes: usize) -> FunctionResult<()> {
        Ok(self.session.runtime.budget.text(bytes)?)
    }
    /// Check growing text capacity when copied bytes are charged separately.
    pub(crate) fn check_text_capacity(&self, bytes: usize) -> FunctionResult<()> {
        Ok(self.session.runtime.budget.check_text(bytes)?)
    }
    /// Check output capacity when materialization work is charged separately.
    pub(crate) fn check_values_capacity(&self, elements: usize) -> FunctionResult<()> {
        Ok(self.session.runtime.budget.check_allocation(elements)?)
    }

    /// Charge helper work against active run and formula limits.
    pub(crate) fn charge_work(&mut self, units: u64) -> FunctionResult<()> {
        Ok(self
            .session
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, units)?)
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
    fn copy_cell(budget: &mut BudgetMeter, value: &CellValue) -> EvalResult<CellValue> {
        if let Some(text) = value.text_ref() {
            budget.text(text.storage_bytes())?;
            budget.allocation(text.len_units())?;
        }
        Ok(value.clone())
    }
    /// Clone a value only after admitting its text, array, and reference storage.
    pub(crate) fn copy_value(budget: &mut BudgetMeter, value: &Value) -> EvalResult<Value> {
        Ok(match value {
            Value::Missing => Value::Missing,
            Value::Scalar(value) => Value::Scalar(Self::copy_cell(budget, value)?),
            Value::SelectedScalar(value, origin) => {
                Value::SelectedScalar(Self::copy_cell(budget, value)?, *origin)
            }
            Value::SignedRefs(areas) => {
                budget.allocation(areas.len())?;
                Value::SignedRefs(areas.clone())
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                budget.allocation(areas.len())?;
                if matches!(value, Value::SheetSpan(_)) {
                    Value::SheetSpan(areas.clone())
                } else {
                    Value::Refs(areas.clone())
                }
            }
            Value::Array(rows) => {
                budget.allocation(rows.iter().map(Vec::len).sum())?;
                Value::Array(crate::ArrayView::anchored(
                    rows.iter()
                        .map(|row| {
                            row.iter()
                                .map(|value| Self::copy_cell(budget, value))
                                .collect()
                        })
                        .collect::<EvalResult<_>>()?,
                    rows.anchor,
                ))
            }
        })
    }
    /// Resolve one deferred argument and inspect its channel without scalarizing it.
    pub(crate) fn kind(&mut self, index: usize) -> FunctionResult<OperandKind> {
        self.prepare(index)?;
        Ok(self.operand(index).ok_or(VALUE_ERROR)?.kind())
    }
    /// Return a budgeted copy of one resolved operand with omission metadata intact.
    pub(crate) fn argument(&mut self, index: usize) -> FunctionResult<HostOperand> {
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
            value: Self::copy_value(&mut runtime.budget, &operand.value)?,
            missing: operand.missing,
            array_missing: operand.array_missing.clone(),
        })
    }
    /// Implicit intersection is performed only on this request. Errors stay data here.
    pub(crate) fn scalar_data(
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
                Self::copy_cell(&mut runtime.budget, value)?.into(),
                OperandOrigin::Direct,
            ),
            Value::SelectedScalar(value, origin) => {
                (Self::copy_cell(&mut runtime.budget, value)?.into(), *origin)
            }
            Value::Array(rows) => (
                Self::copy_cell(
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
            value: self.session.scalar(self.address, value)?,
            origin,
            missing: false,
        })
    }
    /// Project one scalar and promote spreadsheet errors into function failures.
    pub(crate) fn scalar(
        &mut self,
        index: usize,
        policy: MissingPolicy,
    ) -> FunctionResult<CellValue> {
        let value = self.scalar_data(index, policy)?.value;
        if let Error(error) = value {
            Err(error.into())
        } else {
            Ok(value)
        }
    }
    /// Reject a helper's unattributed internal resource exhaustion operationally.
    pub(crate) fn budget_exceeded(&self) -> EvaluationFailure {
        self.capability_failure(
            CapabilityId::FunctionAdapter,
            "helper exhausted an internal budget without reporting its requested count",
        )
    }
    /// Record formatting consumption and copy explicitly configured currency settings.
    pub(crate) fn currency_context(
        &mut self,
    ) -> FunctionResult<crate::adapters::financial::policy::CurrencyContext> {
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
    use poirot_coercionv2::compatibility::MissingScalarPolicy;
    let policy = match policy {
        MissingPolicy::Blank => MissingScalarPolicy::Blank,
        #[cfg(test)]
        MissingPolicy::Zero => MissingScalarPolicy::Zero,
        #[cfg(test)]
        MissingPolicy::Reject => MissingScalarPolicy::Reject,
    };
    poirot_coercionv2::compatibility::coercev2_missing_scalar(policy).map_err(Into::into)
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
