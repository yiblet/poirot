//! Statistical aggregates over one-shot arguments and checked host folds.
use crate::adapters::aggregate::{self, AggregatePolicy};
use crate::adapters::elementwise::{
    Variadic, WholeCollection,
    collections::{self, CollectionKernel},
};
use crate::host::{CallHost, HostOperand, NumericPolicy, OperandOrigin, ScalarOperand};
use crate::*;
use poirot_functions_statistics::{NumericReduction, NumericReductionState};
use std::{collections::TryReserveError, ops::ControlFlow};

/// Numeric and counting state retained across aggregate arguments.
struct State {
    reduction: Option<NumericReductionState>,
    numbers: Vec<f64>,
    count: u64,
    error: Option<CellError>,
    special: bool,
    direct: Option<ScalarOperand>,
    allocation_failed: bool,
}

impl State {
    /// Construct the fixed-scratch reducer or sample vector selected by a function.
    fn new(function: StatisticsFunction) -> Self {
        use StatisticsFunction::*;
        let operation = match function {
            Sum => Some(NumericReduction::Sum),
            SumSq => Some(NumericReduction::SumSq),
            Average => Some(NumericReduction::Average),
            Min => Some(NumericReduction::Minimum),
            Max => Some(NumericReduction::Maximum),
            Product => Some(NumericReduction::Product),
            _ => None,
        };
        Self {
            reduction: operation.map(NumericReductionState::new),
            numbers: Vec::new(),
            count: 0,
            error: None,
            special: false,
            direct: None,
            allocation_failed: false,
        }
    }

    /// Fold one already read member without invoking another host service.
    fn consume(&mut self, function: StatisticsFunction, member: ScalarOperand, selected: bool) {
        use StatisticsFunction::*;
        if self.error.is_some() || self.special || self.allocation_failed {
            return;
        }
        let indirect = !selected && member.origin != OperandOrigin::Direct;
        if !indirect {
            self.direct = Some(member);
            return;
        }
        match function {
            CountA => self.count += u64::from(member.value != Blank),
            Count => self.count += u64::from(matches!(member.value, Number(_))),
            _ => match member.value {
                Error(error) => self.error = Some(error),
                Number(number) => self.push(function, number),
                Blank | Boolean(_) | Text(_) | CellValue::TextUtf16(_) => {}
            },
        }
    }

    /// Accumulate a numeric member under its source compatibility policy.
    fn push(&mut self, function: StatisticsFunction, number: f64) {
        if function != StatisticsFunction::Sum && !number.is_finite() {
            self.special = true;
        } else if let Some(reduction) = &mut self.reduction {
            reduction.push(number);
        } else {
            if self.numbers.len() == self.numbers.capacity() && self.numbers.try_reserve(1).is_err()
            {
                self.allocation_failed = true;
                return;
            }
            // SAFETY: The local capacity check or successful reservation provides one numeric slot.
            self.numbers.push(number);
        }
    }
}

/// Resolve a direct or selected scalar after its eager operand body completes.
fn direct(
    host: &mut CallHost<'_, '_>,
    function: StatisticsFunction,
    state: &mut State,
) -> EvalResult<()> {
    let Some(member) = state.direct.take() else {
        return Ok(());
    };
    if function == StatisticsFunction::CountA {
        state.count += u64::from(
            member.value != Blank || (!member.missing && member.origin == OperandOrigin::Direct),
        );
        return Ok(());
    }
    if function == StatisticsFunction::Count {
        if matches!(member.value, Number(_))
            || (!member.missing && member.origin == OperandOrigin::Direct && member.value == Blank)
            || (!member.missing
                && !matches!(member.value, Blank | Error(_))
                && semantic(host.coercev2_scalar_number(&member.value, NumericPolicy::PinnedPoi))?
                    .is_ok())
        {
            state.count += 1;
        }
        return Ok(());
    }
    if let Error(error) = member.value {
        state.error = Some(error);
        return Ok(());
    }
    let value =
        if !member.missing && member.origin == OperandOrigin::Direct && member.value == Blank {
            Number(0.)
        } else {
            member.value
        };
    if value == Blank {
        return Ok(());
    }
    if function != StatisticsFunction::Sum && matches!(value, Number(number) if !number.is_finite())
    {
        state.special = true;
        return Ok(());
    }
    let number = if function == StatisticsFunction::Sum {
        match value {
            Number(number) => Ok(number),
            _ => host.coercev2_scalar_number(&value, NumericPolicy::PinnedPoi),
        }
    } else {
        host.coercev2_scalar_number(&value, NumericPolicy::PinnedPoi)
    };
    match semantic(number)? {
        Ok(number) => {
            if state.reduction.is_none() {
                host.check_values(state.numbers.len().saturating_add(1))?;
                host.charge_kind(WorkKind::Materialization, 1)?;
                state
                    .numbers
                    .try_reserve(1)
                    .map_err(|_| host.allocation_failure())?;
            }
            state.push(function, number);
        }
        Err(error) => state.error = Some(error),
    }
    Ok(())
}

/// Finish a completed aggregate with the independent statistics kernels.
fn finish(function: StatisticsFunction, mut state: State) -> EvalResult<Value> {
    if matches!(
        function,
        StatisticsFunction::Count | StatisticsFunction::CountA
    ) {
        return Ok(Number(state.count as f64).into());
    }
    if let Some(reduction) = state.reduction {
        return Ok(match reduction.finish() {
            Some(value) => numeric_outcome::coerce_checked_numeric_result(value).into(),
            None => err(Div0),
        });
    }
    let result = match function {
        StatisticsFunction::Median => poirot_functions_statistics::median(&mut state.numbers),
        StatisticsFunction::Stdev
        | StatisticsFunction::StdevP
        | StatisticsFunction::Var
        | StatisticsFunction::VarP => {
            use poirot_functions_statistics::{Aggregate, SamplePolicy, aggregate_with_policy};
            let operation = match function {
                StatisticsFunction::Stdev => Aggregate::StdevSample,
                StatisticsFunction::StdevP => Aggregate::StdevPopulation,
                StatisticsFunction::Var => Aggregate::VarianceSample,
                _ => Aggregate::VariancePopulation,
            };
            aggregate_with_policy(operation, &state.numbers, SamplePolicy::ExcelDocumentation)
        }
        _ => return Ok(err(VALUE_ERROR)),
    };
    Ok(match result {
        Ok(value) => numeric_outcome::coerce_checked_numeric_result(value).into(),
        Err(error) => err(error),
    })
}

/// Policy for numeric and count aggregates sharing one source traversal.
struct StatisticsAggregate {
    function: StatisticsFunction,
    state: State,
    before_numbers: usize,
}

impl StatisticsAggregate {
    /// Select the accumulator and origin-sensitive admission contract.
    fn new(function: StatisticsFunction) -> Self {
        Self {
            function,
            state: State::new(function),
            before_numbers: 0,
        }
    }
}

impl AggregatePolicy for StatisticsAggregate {
    fn before(&mut self, host: &CallHost<'_, '_>, operand: &HostOperand) -> EvalResult<()> {
        if self.function == StatisticsFunction::Sum
            && matches!(operand.value(), Value::SignedRefs(areas) if areas.len() > 1)
        {
            return Err(
                host.known_source_exception(KnownSourceException::AggregateReferenceListConversion)
            );
        }
        self.before_numbers = self.state.numbers.len();
        Ok(())
    }

    fn reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        if self.state.reduction.is_some()
            || matches!(
                self.function,
                StatisticsFunction::Count | StatisticsFunction::CountA
            )
        {
            Ok(())
        } else {
            self.state.numbers.try_reserve(additional)
        }
    }

    fn consume(&mut self, member: ScalarOperand, selected: bool) -> ControlFlow<()> {
        self.state.consume(self.function, member, selected);
        if self.state.error.is_some() || self.state.special || self.state.allocation_failed {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn after(&mut self, host: &mut CallHost<'_, '_>) -> EvalResult<Option<Value>> {
        if self.state.allocation_failed {
            return Err(host.allocation_failure());
        }
        if self.state.numbers.len() > self.before_numbers {
            host.charge_kind(
                WorkKind::Materialization,
                (self.state.numbers.len() - self.before_numbers) as u64,
            )?;
        }
        if self.state.error.is_none() && !self.state.special {
            direct(host, self.function, &mut self.state)?;
        }
        if self.state.allocation_failed {
            return Err(host.allocation_failure());
        }
        if let Some(error) = self.state.error {
            return Ok(Some(err(error)));
        }
        if self.state.special {
            return Err(host.capability_failure(
                CapabilityId::FunctionAdapter,
                "pinned special-number aggregate algorithm is not implemented for this handler",
            ));
        }
        Ok(None)
    }

    fn finish(self) -> EvalResult<Value> {
        finish(self.function, self.state)
    }
}

/// Count logical blank positions while preserving sparse reference geometry.
struct CountBlank;
impl CollectionKernel for CountBlank {
    type Signature = Variadic<WholeCollection>;
    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        let count =
            collections::fold_sources(host, arguments.iter(), 0u64, |host, count, operand| {
                if let Some(blanks) = host.countblank_references(operand.value())? {
                    *count = count
                        .checked_add(blanks)
                        .ok_or_else(|| host.size_overflow())?;
                    Ok(())
                } else {
                    let value = host.select_legacy_scalar(operand.into_value())?;
                    Err(match value {
                        Error(error) => error.into(),
                        _ => VALUE_ERROR.into(),
                    })
                }
            })?;
        Ok(Number(count as f64).into())
    }
}

/// Execute statistical aggregates with typed admission and checked source folding.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: StatisticsFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if function == StatisticsFunction::CountBlank {
        let mut host = CallHost::new_inheriting(session, sheet, address, arguments);
        match semantic(collections::execute(&mut host, address, CountBlank))? {
            Ok(value) => Ok(value),
            Err(error) => Ok(err(error)),
        }
    } else {
        aggregate::execute(
            session,
            sheet,
            address,
            arguments,
            StatisticsAggregate::new(function),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_buffer_reserves_locally_without_caller_admission() {
        let mut state = State::new(StatisticsFunction::Median);
        state.push(StatisticsFunction::Median, 2.);
        assert_eq!(state.numbers, [2.]);
        assert!(!state.allocation_failed);
    }

    #[test]
    fn impossible_reservation_preserves_existing_numeric_state() {
        let mut aggregate = StatisticsAggregate::new(StatisticsFunction::Median);
        aggregate.state.push(StatisticsFunction::Median, 2.);
        assert!(aggregate.reserve(usize::MAX).is_err());
        assert_eq!(aggregate.state.numbers, [2.]);
    }
}
