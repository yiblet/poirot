//! Typed conditional function IDs and scalar domain conversion.
use super::{FunctionResult, host::CallHost};
use poirot_functions_criteria::{Cell, Error};
use poirot_model::{CellError, CellValue};
/// Conditional aggregation operations shared by criteria bridges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// Count cells matching one criterion.
    CountIf,
    /// Sum cells selected by one criterion.
    SumIf,
    /// Average cells selected by one criterion.
    AverageIf,
    /// Count rows matching multiple criteria.
    CountIfs,
    /// Sum rows matching multiple criteria.
    SumIfs,
    /// Average rows matching multiple criteria.
    AverageIfs,
    /// Maximum among rows matching multiple criteria.
    MaxIfs,
    /// Minimum among rows matching multiple criteria.
    MinIfs,
    /// Aggregate with subtotal and row-visibility filtering.
    Subtotal,
}
/// Convert an admitted engine scalar into the criteria helper's value domain.
pub(crate) fn cell(host: &mut CallHost<'_, '_>, v: &CellValue) -> FunctionResult<Cell> {
    Ok(match v {
        CellValue::Blank => Cell::Blank,
        CellValue::Number(n) => Cell::Number(*n),
        CellValue::Boolean(b) => Cell::Bool(*b),
        CellValue::Text(s) => Cell::Text(s.clone()),
        CellValue::TextUtf16(_) => Cell::Text(host.scalar_text(v)?),
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
pub(crate) fn error(e: Error) -> CellValue {
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
