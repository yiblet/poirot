//! Logical reduction policy over shared checked aggregate traversal.
use crate::eval::adapters::aggregate::{self, AggregatePolicy};
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::host::{CallHost, ScalarOperand};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{OperandOrigin, VALUE_ERROR, Value, core_projection, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellValue::{Blank, Boolean, Text};
use crate::model::{CellAddress, CellError, CellValue};
use poirot_functions::logical::{LogicalReduction, LogicalReductionState};
use std::{collections::TryReserveError, ops::ControlFlow};

/// AND, OR, or XOR policy used by shared aggregate traversal.
/// It applies origin-sensitive boolean admission and visits operands until the first cell error, even when the boolean result is already decided.
struct LogicalAggregate {
    function: LogicalFunction,
    truth: LogicalReductionState,
    error: Option<CellError>,
}

impl LogicalAggregate {
    /// Select conjunction, disjunction, or parity without member storage.
    fn new(function: LogicalFunction) -> Self {
        let reduction = match function {
            LogicalFunction::And => LogicalReduction::And,
            LogicalFunction::Xor => LogicalReduction::Xor,
            _ => LogicalReduction::Or,
        };
        Self {
            function,
            truth: LogicalReductionState::new(reduction),
            error: None,
        }
    }
}

impl AggregatePolicy for LogicalAggregate {
    fn reserve(&mut self, _additional: usize) -> Result<(), TryReserveError> {
        Ok(())
    }

    fn consume(&mut self, member: ScalarOperand, selected: bool) -> ControlFlow<()> {
        if self.error.is_some() {
            return ControlFlow::Break(());
        }
        let collection = if self.function == LogicalFunction::Xor {
            member.origin != OperandOrigin::Direct
        } else {
            !selected && member.origin != OperandOrigin::Direct
        };
        if collection && matches!(member.value, Text(_) | CellValue::TextUtf16(_) | Blank) {
            return ControlFlow::Continue(());
        }
        match core_projection::coerce_operand_boolean(&member.value) {
            Ok(boolean) => self.truth.push(boolean),
            Err(error) => self.error = Some(error),
        }
        if self.error.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn after(&mut self, _host: &mut CallHost<'_, '_>) -> EvalResult<Option<Value>> {
        Ok(self.error.map(err))
    }

    fn finish(self) -> EvalResult<Value> {
        Ok(match self.truth.finish() {
            Some(value) => Boolean(value).into(),
            None => err(VALUE_ERROR),
        })
    }
}

/// Execute logical reductions using one typed policy for all three functions.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: LogicalFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    aggregate::execute(
        session,
        sheet,
        address,
        arguments,
        LogicalAggregate::new(function),
    )
}
