//! Pure logical scalar algorithms. Evaluation and branch selection belong to the host.
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
