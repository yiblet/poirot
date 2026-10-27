//! Conversion between engine scalars and the criteria helper's value domain.

use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::values::FunctionResult;
use crate::model::{CellError, CellValue};
use poirot_functions::criteria::{Cell, Error};

/// Convert an admitted engine scalar into the criteria helper's value domain.
pub(super) fn cell(host: &mut CallHost<'_, '_>, v: &CellValue) -> FunctionResult<Cell> {
    Ok(match v {
        CellValue::Blank => Cell::Blank,
        CellValue::Number(n) => Cell::Number(*n),
        CellValue::Boolean(b) => Cell::Bool(*b),
        CellValue::Text(s) => Cell::Text(s.clone()),
        CellValue::TextUtf16(_) => Cell::Text(ArgumentCoercion::new(host).coerce_scalar_text(v)?),
        CellValue::Error(e) => Cell::Error(match e {
            CellError::Null => Error::Null,
            CellError::Div0 => Error::DivZero,
            CellError::Value => Error::Value,
            CellError::Ref => Error::Ref,
            CellError::Name => Error::Name,
            CellError::Num => Error::Num,
            CellError::NA => Error::Na,
            CellError::GettingData | CellError::Spill | CellError::Calc => return Err((*e).into()),
        }),
    })
}

/// Map a criteria helper error back to a spreadsheet error value.
pub(super) fn error(e: Error) -> CellValue {
    CellValue::Error(match e {
        Error::Null => CellError::Null,
        Error::DivZero => CellError::Div0,
        Error::Value => CellError::Value,
        Error::Ref => CellError::Ref,
        Error::Name => CellError::Name,
        Error::Num => CellError::Num,
        Error::Na => CellError::NA,
    })
}
