//! Scalar helpers for spreadsheet logical functions.
//!
//! Use [`coercev2_logical_test`](crate::logical::coercev2_logical_test) to convert a value into a logical condition and
//! [`error_type`](crate::logical::error_type) to obtain an `ERROR.TYPE` code.
//!
//! The caller selects branches and evaluates arguments, including short-circuit
//! behavior. This crate does not evaluate complete formulas.
use crate::scalar::{CellError, CellValue};

/// Coerce an admitted scalar into a logical condition under the kernel rules.
pub fn coercev2_logical_test(value: &CellValue) -> Result<bool, CellError> {
    crate::coercion::compatibility::coercev2_logical_test(value)
}

/// Return the documented ERROR.TYPE code, including GETTING_DATA as code eight.
pub fn error_type(error: CellError) -> Option<u8> {
    match error {
        CellError::Null => Some(1),
        CellError::Div0 => Some(2),
        CellError::Value => Some(3),
        CellError::Ref => Some(4),
        CellError::Name => Some(5),
        CellError::Num => Some(6),
        CellError::NA => Some(7),
        CellError::GettingData => Some(8),
        CellError::Spill | CellError::Calc => None,
    }
}

/// Ordered logical reduction selected after caller-owned coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalReduction {
    /// Require every admitted value to be true.
    And,
    /// Require at least one admitted value to be true.
    Or,
    /// Require an odd number of admitted true values.
    Xor,
}

/// Incremental AND, OR, or XOR reduction over already coerced values.
/// It retains whether any value was admitted and does not short-circuit later pushes.
#[derive(Clone, Copy, Debug)]
pub struct LogicalReductionState {
    operation: LogicalReduction,
    value: Option<bool>,
}
impl LogicalReductionState {
    /// Start an empty logical reduction.
    pub fn new(operation: LogicalReduction) -> Self {
        Self {
            operation,
            value: None,
        }
    }

    /// Consume a coerced value without skipping subsequent operands.
    pub fn push(&mut self, value: bool) {
        self.value = Some(match (self.operation, self.value) {
            (_, None) => value,
            (LogicalReduction::And, Some(previous)) => previous & value,
            (LogicalReduction::Or, Some(previous)) => previous | value,
            (LogicalReduction::Xor, Some(previous)) => previous ^ value,
        });
    }

    /// Return None when no logical values were admitted.
    pub fn finish(self) -> Option<bool> {
        self.value
    }
}

#[cfg(test)]
mod reduction_tests {
    use super::{LogicalReduction, LogicalReductionState};

    #[test]
    fn empty_and_ordered_logical_reductions() {
        for operation in [LogicalReduction::And, LogicalReduction::Or] {
            let mut state = LogicalReductionState::new(operation);
            assert_eq!(state.finish(), None);
            state.push(false);
            assert_eq!(state.finish(), Some(false));
            state.push(true);
            assert_eq!(state.finish(), Some(operation == LogicalReduction::Or));
            state.push(false);
            assert_eq!(state.finish(), Some(operation == LogicalReduction::Or));
        }
    }
}
