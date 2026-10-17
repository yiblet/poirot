//! Logical/information execution policies sourced from pinned Apache POI.
//! The evaluation port retains deferred operands and reference-valued results.
use poirot_functions_core::{CellError, CellValue};

/// Lazy selection and logical inspection operations handled by this adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalFunction {
    /// ERROR.TYPE error classification.
    ErrorType,
    /// First matching condition/result pair.
    Ifs,
    /// Even-integer test.
    IsEven,
    /// Odd-integer test.
    IsOdd,
    /// Reference-channel test without dereferencing contents.
    IsRef,
    /// Match an expression against cases and select one result.
    Switch,
}

/// Cell failures may be handled by spreadsheet policies; operational failures
/// (budget, cancellation, host errors) must propagate unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure<E> {
    /// Catchable spreadsheet error.
    Cell(CellError),
    /// Host operational abort.
    Operational(E),
    /// Errortype.java throws for POI-internal/nontraditional error codes.
    UnsupportedPoiError(CellError),
}

/// A callback into the evaluator, with no dependency on its AST or references.
/// Every indexed method evaluates only the named operand, once on demand.
/// `scalar` follows OperandResolver.getSingleValue (including source-cell
/// implicit intersection) and returns spreadsheet errors as Failure::Cell.
/// Missing operands must retain their evaluator-specific coercion behavior;
/// they must not silently become blank in numeric/logical scalar contexts.
pub trait EvaluationPort {
    /// Engine value channel, preserving references and arrays.
    type Output;
    /// Host operational failure type.
    type OperationalError;

    /// Resolve one argument with caller-cell scalar intersection rules.
    fn scalar(&mut self, index: usize) -> Result<CellValue, Failure<Self::OperationalError>>;
    /// Evaluate the original argument without scalarizing it. The returned
    /// opaque value preserves reference/area/list identity and blank/missing.
    fn argument(&mut self, index: usize) -> Result<Self::Output, Failure<Self::OperationalError>>;
    /// Evaluate the original argument and classify RefEval/AreaEval/RefListEval.
    /// A scalar error is a non-reference, not a Failure::Cell.
    fn is_reference(&mut self, index: usize) -> Result<bool, Failure<Self::OperationalError>>;
    /// OperandResolver numeric coercion followed by ExcelArithmetic.truncate.
    /// The 15-digit Excel view is required (2.9999999999999996 -> 3).
    /// This callback allows the common numeric policy to own conversion rather
    /// than duplicating approximate floating-point rounding in this family.
    fn coercev2_truncated_number(
        &mut self,
        index: usize,
    ) -> Result<f64, Failure<Self::OperationalError>>;
    /// POI EqualEval comparison: case-insensitive text, cross-type ordering,
    /// blank comparison and NumberComparer numeric precision. Cell errors must
    /// be Failure::Cell; operational failures must remain operational.
    fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> Result<bool, Failure<Self::OperationalError>>;
    /// Wrap a scalar result in the engine's value channel.
    fn value(value: CellValue) -> Self::Output;
}

/// Validated shape of a lazy selection function. No values are evaluated here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPlan(SelectionKind);

/// Conditional-selection rule used by the logical adapter's lazy argument plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionKind {
    Ifs {
        pairs: usize,
    },
    Switch {
        pairs: usize,
        default: Option<usize>,
    },
}
impl SelectionPlan {
    /// Validate condition/result pairing for an IFS call.
    pub fn ifs(arguments: usize) -> Result<Self, CellError> {
        if !arguments.is_multiple_of(2) {
            return Err(CellError::Value);
        }
        Ok(Self(SelectionKind::Ifs {
            pairs: arguments / 2,
        }))
    }
    /// Validate SWITCH cases and locate its optional default operand.
    pub fn switch(arguments: usize) -> Result<Self, CellError> {
        if arguments < 3 {
            return Err(CellError::NA);
        }
        Ok(Self(SelectionKind::Switch {
            pairs: (arguments - 1) / 2,
            default: arguments.is_multiple_of(2).then_some(arguments - 1),
        }))
    }

    /// Return only the selected index. Skipped result operands are never
    /// evaluated, and resolving the selected output belongs to the adapter.
    pub fn select<P: EvaluationPort>(
        self,
        port: &mut P,
    ) -> Result<usize, Failure<P::OperationalError>> {
        match self.0 {
            SelectionKind::Ifs { pairs } => {
                for pair in 0..pairs {
                    if coercev2_logical_test(&port.scalar(pair * 2)?).map_err(Failure::Cell)? {
                        return Ok(pair * 2 + 1);
                    }
                }
                Err(Failure::Cell(CellError::NA))
            }
            SelectionKind::Switch { pairs, default } => {
                let expression = switch_scalar(port.scalar(0))?;
                for pair in 0..pairs {
                    let value = switch_scalar(port.scalar(pair * 2 + 1))?;
                    // EqualEval can return ErrorEval rather than throwing;
                    // POI treats that outcome as a failed match and continues.
                    match port.equal(&expression, &value) {
                        Ok(true) => return Ok(pair * 2 + 2),
                        Ok(false) | Err(Failure::Cell(_)) => {}
                        Err(error) => return Err(error),
                    }
                }
                default.ok_or(Failure::Cell(CellError::NA))
            }
        }
    }
}

fn switch_scalar<E>(value: Result<CellValue, Failure<E>>) -> Result<CellValue, Failure<E>> {
    match value {
        Err(Failure::Cell(_)) | Ok(CellValue::Error(_)) => Err(Failure::Cell(CellError::NA)),
        other => other,
    }
}

/// POI coerceValueToBoolean(value, false): whitespace is not stripped from
/// strings, numbers use nonzero truth, and blank is false.
pub use poirot_functions_logical::{coercev2_logical_test, error_type};

/// Actual family adapter: validates shape, executes deferred callbacks, then
/// returns the selected reference directly or constructs a scalar result.
pub fn evaluate<P: EvaluationPort>(
    function: LogicalFunction,
    arguments: usize,
    port: &mut P,
) -> Result<P::Output, Failure<P::OperationalError>> {
    match function {
        LogicalFunction::Ifs | LogicalFunction::Switch => {
            let plan = if function == LogicalFunction::Ifs {
                SelectionPlan::ifs(arguments)
            } else {
                SelectionPlan::switch(arguments)
            }
            .map_err(Failure::Cell)?;
            let selected = plan.select(port)?;
            port.argument(selected)
        }
        _ if arguments != 1 => Err(Failure::Cell(CellError::Value)),
        LogicalFunction::ErrorType => {
            let error = match port.scalar(0) {
                Err(Failure::Cell(error)) | Ok(CellValue::Error(error)) => error,
                Ok(_) => return Err(Failure::Cell(CellError::NA)),
                Err(error) => return Err(error),
            };
            let code = error_type(error).ok_or(Failure::UnsupportedPoiError(error))?;
            Ok(P::value(CellValue::Number(f64::from(code))))
        }
        LogicalFunction::IsEven | LogicalFunction::IsOdd => {
            let number = port.coercev2_truncated_number(0)?;
            let even = number.abs() % 2.0 == 0.0;
            Ok(P::value(CellValue::Boolean(
                even == (function == LogicalFunction::IsEven),
            )))
        }
        LogicalFunction::IsRef => Ok(P::value(CellValue::Boolean(port.is_reference(0)?))),
    }
}

#[cfg(test)]
mod tests;
