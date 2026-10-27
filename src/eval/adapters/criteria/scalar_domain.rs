//! Conversion between engine scalars and the criteria helper's value domain.

use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::values::FunctionResult;
use crate::model::{CellError, CellValue};
use poirot_functions::criteria::{Cell, CriteriaError};

/// Convert an admitted engine scalar into the criteria helper's value domain.
pub(super) fn cell(host: &mut CallHost<'_, '_>, v: &CellValue) -> FunctionResult<Cell> {
    Ok(match v {
        CellValue::Blank => Cell::Blank,
        CellValue::Number(n) => Cell::Number(*n),
        CellValue::Boolean(b) => Cell::Bool(*b),
        CellValue::Text(s) => Cell::Text(s.clone()),
        CellValue::TextUtf16(_) => Cell::Text(ArgumentCoercion::new(host).coerce_scalar_text(v)?),
        CellValue::Error(e) => Cell::Error(match e {
            CellError::Null => CriteriaError::Null,
            CellError::Div0 => CriteriaError::DivZero,
            CellError::Value => CriteriaError::Value,
            CellError::Ref => CriteriaError::Ref,
            CellError::Name => CriteriaError::Name,
            CellError::Num => CriteriaError::Num,
            CellError::NA => CriteriaError::Na,
            CellError::GettingData | CellError::Spill | CellError::Calc => return Err((*e).into()),
        }),
    })
}

/// Map a criteria helper error back to a spreadsheet error value.
pub(super) fn error(e: CriteriaError) -> CellValue {
    CellValue::Error(match e {
        CriteriaError::Null => CellError::Null,
        CriteriaError::DivZero => CellError::Div0,
        CriteriaError::Value => CellError::Value,
        CriteriaError::Ref => CellError::Ref,
        CriteriaError::Name => CellError::Name,
        CriteriaError::Num => CellError::Num,
        CriteriaError::Na => CellError::NA,
    })
}
