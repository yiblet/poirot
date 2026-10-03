//! Pure logical scalar algorithms. Evaluation and branch selection belong to the host.
use poirot_functions_core::{CellError, CellValue};

/// Coerce an admitted scalar into a logical condition under the kernel rules.
pub fn logical_test(value: &CellValue) -> Result<bool, CellError> {
    if let Some(text) = value.text_ref() {
        let equal = |expected: &[u8]| {
            let length = match text {
                poirot_functions_core::TextRef::Utf8(t) => t.len(),
                poirot_functions_core::TextRef::Utf16(u) => u.len(),
            };
            length == expected.len()
                && text
                    .units()
                    .zip(expected)
                    .all(|(unit, byte)| unit <= 127 && (unit as u8).eq_ignore_ascii_case(byte))
        };
        return if equal(b"true") {
            Ok(true)
        } else if equal(b"false") {
            Ok(false)
        } else {
            Err(CellError::Value)
        };
    }
    match value {
        CellValue::Blank => Ok(false),
        CellValue::Boolean(value) => Ok(*value),
        CellValue::Number(value) if value.is_nan() => Err(CellError::Value),
        CellValue::Number(value) => Ok(*value != 0.0),
        CellValue::Error(error) => Err(*error),
        _ => Err(CellError::Value),
    }
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
