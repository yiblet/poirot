//! Logical selection policies over the shared deferred executor.
use crate::eval::adapters::elementwise::selection::{
    self, CandidateRoles, SelectionTest, Selector, VariadicLayout, VariadicSelector,
};
use crate::eval::adapters::elementwise::{
    Broadcast, Deferred, Eager, FixedScalar, Signature, WithDemand,
};
use crate::eval::functions::registry::{FunctionId, LogicalFunction};
use crate::eval::host::CallHost;
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::{VALUE_ERROR, Value, core_projection, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::NA;
use crate::model::CellValue::{Blank, Boolean, Error};
use crate::model::{CellAddress, CellError, CellValue};

/// CHOOSE policy for the shared deferred branch selector.
/// A numeric position chooses one result argument, leaving other branches unforced.
struct Choose;

impl VariadicSelector for Choose {
    fn omitted(&self) -> Value {
        Blank.into()
    }

    fn layout(&self, count: usize) -> VariadicLayout {
        VariadicLayout {
            first: 0,
            end: 1,
            step: 1,
            default: None,
            candidates: CandidateRoles::Positions,
            results: 1..count,
        }
    }

    fn test(&self, _: &CellValue, _: Option<&CellValue>) -> SelectionTest {
        SelectionTest::Index
    }

    fn position(&self, index: f64, count: usize) -> Result<usize, CellError> {
        if index >= 1. && index.trunc() < count as f64 {
            Ok(index as usize)
        } else {
            Err(VALUE_ERROR)
        }
    }
}

/// IFS policy that selects the first true condition through shared deferred branch traversal.
/// Condition coercion errors propagate before any later condition or result is read.
struct Ifs;

impl VariadicSelector for Ifs {
    fn layout(&self, count: usize) -> VariadicLayout {
        VariadicLayout {
            first: 0,
            end: count,
            step: 2,
            default: None,
            candidates: CandidateRoles::Conditions,
            results: 1..count,
        }
    }

    fn test(&self, value: &CellValue, _: Option<&CellValue>) -> SelectionTest {
        SelectionTest::Match(poirot_functions::logical::coerce_logical_test(value))
    }
}

/// SWITCH policy that selects the first equal case through shared deferred branch traversal.
/// Expression and case errors become #N/A, while failed equality conversion counts as a nonmatch.
struct Switch;

impl VariadicSelector for Switch {
    fn expression_error(&self, _: CellError) -> CellError {
        NA
    }

    fn comparison(&self, result: Result<bool, CellError>) -> Result<bool, CellError> {
        Ok(result.unwrap_or(false))
    }

    fn layout(&self, count: usize) -> VariadicLayout {
        let default = count.is_multiple_of(2).then(|| count.saturating_sub(1));
        VariadicLayout {
            first: 1,
            end: default.unwrap_or(count),
            step: 2,
            default,
            candidates: CandidateRoles::Cases,
            results: 2..default.unwrap_or(count),
        }
    }

    fn test(&self, value: &CellValue, expression: Option<&CellValue>) -> SelectionTest {
        if matches!(value, Error(_)) || matches!(expression, Some(Error(_))) {
            SelectionTest::Match(Err(NA))
        } else {
            SelectionTest::Equal
        }
    }
}

/// Execute a validated variadic logical selector with shared deferred machinery.
pub(super) fn execute_variadic(
    host: &mut CallHost<'_, '_>,
    function: LogicalFunction,
) -> EvalResult<Value> {
    match function {
        LogicalFunction::Choose => selection::variadic(host, Choose),
        LogicalFunction::Ifs if host.len().is_multiple_of(2) => selection::variadic(host, Ifs),
        LogicalFunction::Ifs => Ok(err(VALUE_ERROR)),
        LogicalFunction::Switch => selection::variadic(host, Switch),
        _ => Err(EvaluationError::StalePlan),
    }
}

/// Execute IF while evaluating only demanded branches and members.
pub(super) fn execute_if(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Logical(LogicalFunction::If)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_inheriting(session, sheet, at, args);
    let (condition, when_true, when_false) = <(
        WithDemand<FixedScalar, Eager>,
        Deferred<Broadcast>,
        Deferred<Broadcast>,
    ) as Signature>::bind(host.len());
    selection::execute(&mut host, condition, [when_true, when_false], If)
}

/// IF policy that chooses a deferred result branch from a boolean condition.
/// An absent result defaults to FALSE, while an explicitly omitted result defaults to blank.
struct If;

impl Selector<2> for If {
    fn select(&self, value: &CellValue) -> Result<usize, CellError> {
        core_projection::coerce_operand_boolean(value).map(|value| usize::from(!value))
    }

    fn omitted(&self, absent: bool) -> Value {
        if absent {
            Boolean(false).into()
        } else {
            Blank.into()
        }
    }
}
