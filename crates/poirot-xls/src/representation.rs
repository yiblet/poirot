//! Pure BIFF value and Unicode representation checks.
use crate::{XlsError, fail};
use poirot_model::{CellError, CellValue};
/// Map a supported spreadsheet error to its BIFF cache code.
pub(super) fn error_code(e: CellError) -> Result<u8, XlsError> {
    Ok(match e {
        CellError::Null => 0,
        CellError::Div0 => 7,
        CellError::Value => 15,
        CellError::Ref => 23,
        CellError::Name => 29,
        CellError::Num => 36,
        CellError::NA => 42,
        _ => return Err(fail("modern error code cannot be represented in BIFF8")),
    })
}
/// Encode exact text units using the supported BIFF Unicode representation.
pub(super) fn unicode(s: &str) -> Result<Vec<u16>, XlsError> {
    let units: Vec<_> = s.encode_utf16().collect();
    if units.len() > 32767 {
        return Err(fail("text exceeds Excel's 32767 UTF-16-unit limit"));
    }
    Ok(units)
}
/// Encode a saved formula result without recalculating its source.
pub(super) fn cached(value: &CellValue) -> Result<[u8; 8], XlsError> {
    if let CellValue::Number(n) = value {
        if !n.is_finite() {
            return Err(fail("nonfinite numeric formula cache"));
        }
        return Ok(n.to_le_bytes());
    }
    let (kind, data) = match value {
        CellValue::Blank => (3, 0),
        CellValue::Text(_) | CellValue::TextUtf16(_) => (0, 0),
        CellValue::Boolean(b) => (1, u8::from(*b)),
        CellValue::Error(e) => (2, error_code(*e)?),
        // SAFETY: The Number branch returns above; the remaining explicit arms cover every other CellValue variant.
        _ => unreachable!(),
    };
    Ok([kind, 0, data, 0, 0, 0, 0xff, 0xff])
}
