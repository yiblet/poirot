//! Scalar helpers for spreadsheet logical functions.
//!
//! Use [`coercev2_logical_test`] to convert a value into a logical condition and
//! [`error_type`] to obtain an `ERROR.TYPE` code.
//!
//! The caller selects branches and evaluates arguments, including short-circuit
//! behavior. This crate does not evaluate complete formulas.
use poirot_functions_core::{CellError, CellValue};

/// Coerce an admitted scalar into a logical condition under the kernel rules.
pub fn coercev2_logical_test(value: &CellValue) -> Result<bool, CellError> {
    poirot_coercionv2::compatibility::coercev2_logical_test(value)
}

/// Return the ERROR.TYPE code for a supported spreadsheet error.
pub fn error_type(error: CellError) -> Option<u8> {
    match error {
        CellError::Null => Some(1),
        CellError::Div0 => Some(2),
        CellError::Value => Some(3),
        CellError::Ref => Some(4),
        CellError::Name => Some(5),
        CellError::Num => Some(6),
        CellError::NA => Some(7),
        CellError::GettingData | CellError::Spill | CellError::Calc => None,
    }
}
