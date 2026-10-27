//! Statistical aggregates over one-shot arguments and checked host folds.
use crate::eval::adapters::aggregate::{self, AggregatePolicy};
use crate::eval::adapters::elementwise::{
    Variadic, WholeCollection,
    collections::{self, CollectionKernel},
};
use crate::eval::functions::registry::StatisticsFunction;
use crate::eval::host::{ArgumentCoercion, CallHost, HostOperand, OperandReader, ScalarOperand};
use crate::eval::limits::{CapabilityId, EvalResult, WorkKind};
use crate::eval::session::Session;
use crate::eval::values::core_projection::semantic;
use crate::eval::values::{
    FunctionResult, OperandOrigin, VALUE_ERROR, Value, err, numeric_outcome,
};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Div0;
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::statistics::{NumericReduction, NumericReductionState};
use std::{collections::TryReserveError, ops::ControlFlow};

/// Aggregate progress retained while arguments are visited in source order.
/// The chosen operation uses either a fixed-size reducer or stored numbers, alongside counts and deferred error state.
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
        use StatisticsFunction::{Average, Max, Min, Product, Sum, SumSq};
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
        use StatisticsFunction::{Count, CountA};
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
                && semantic(ArgumentCoercion::new(host).coerce_scalar_number(&member.value))?
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
            poirot_functions::coercion::collections::coerce_aggregate_argument(
                member.value,
                poirot_functions::scalar::Origin::Direct,
            )
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
            _ => ArgumentCoercion::new(host).coerce_scalar_number(&value),
        }
    } else {
        ArgumentCoercion::new(host).coerce_scalar_number(&value)
    };
    match semantic(number)? {
        Ok(number) => {
            if state.reduction.is_none() {
                host.check_values(state.numbers.len().saturating_add(1))?;
                host.charge_kind(WorkKind::Materialization, 1)?;
                state
                    .numbers
                    .try_reserve(1)
                    .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
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
            Some(value) => numeric_outcome::convert_checked_numeric_result(value).into(),
            None => err(Div0),
        });
    }
    let result = match function {
        StatisticsFunction::Median => poirot_functions::statistics::median(&mut state.numbers),
        StatisticsFunction::Stdev
        | StatisticsFunction::StdevP
        | StatisticsFunction::Var
        | StatisticsFunction::VarP => {
            use poirot_functions::statistics::{Aggregate, aggregate};
            let operation = match function {
                StatisticsFunction::Stdev => Aggregate::StdevSample,
                StatisticsFunction::StdevP => Aggregate::StdevPopulation,
                StatisticsFunction::Var => Aggregate::VarianceSample,
                _ => Aggregate::VariancePopulation,
            };
            aggregate(operation, &state.numbers)
        }
        _ => return Ok(err(VALUE_ERROR)),
    };
    Ok(match result {
        Ok(value) => numeric_outcome::convert_checked_numeric_result(value).into(),
        Err(error) => err(error),
    })
}

/// Applies a statistical aggregate's origin-sensitive value rules during shared source traversal.
/// It owns the accumulator and tracks argument boundaries needed by compatibility behavior.
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
    fn before(&mut self, _host: &CallHost<'_, '_>, _operand: &HostOperand) -> EvalResult<()> {
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
                crate::eval::limits::EvaluationReasonError::PinnedSpecialNumberAggregateAlgorithmIsNotImplementedForThisHandler,
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
                if let Some(blanks) =
                    OperandReader::new(host).countblank_references(operand.value())?
                {
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
    fn sum_traverses_signed_reference_unions() {
        use crate::eval::config::EvaluationInputs;
        use crate::eval::values::AreaKind;
        use crate::eval::values::signed_reference::SignedArea;
        use crate::model::Workbook;

        let mut workbook = Workbook::new();
        workbook.add_sheet("Sheet1").unwrap();
        workbook.sheet_mut(0).unwrap().set_value("A1", 2.).unwrap();
        workbook.sheet_mut(0).unwrap().set_value("B1", 7.).unwrap();
        let mut session = Session::with_context(&workbook, EvaluationInputs::default());
        let mut host = CallHost::new_inheriting(
            &mut session,
            0,
            CellAddress { row: 2, col: 2 },
            ArgumentsRef::Empty,
        );
        let operand = HostOperand::from_value(Value::SignedRefs(vec![
            SignedArea::new(AreaKind::Area, 0, (0, 0), (0, 0)).unwrap(),
            SignedArea::new(AreaKind::Area, 0, (0, 0), (1, 1)).unwrap(),
        ]));
        let mut aggregate = StatisticsAggregate::new(StatisticsFunction::Sum);
        aggregate.before(&host, &operand).unwrap();
        let aggregate = host
            .fold_aggregate_operand(
                operand,
                aggregate,
                |state, additional| state.reserve(additional),
                |state, member| {
                    let _ = state.consume(member, false);
                    Ok(())
                },
            )
            .unwrap();
        // The internal signed representation obeys the same 2 + 7 union sum.
        assert!(matches!(
            aggregate.finish().unwrap(),
            Value::Scalar(Number(9.))
        ));
    }

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
