//! Spill reference anchors.
use crate::eval::values::{Area, Value};
use crate::model::CellError;
use crate::model::CellError::Ref;
use crate::model::CellValue::Error;

/// Admit one physical spill anchor while retaining scalar error precedence.
pub(in crate::eval) fn source_anchor(value: &Value) -> Result<Area, CellError> {
    match value {
        Value::Refs(areas) | Value::SheetSpan(areas) => match areas.as_slice() {
            [area] if area.size() == 1 => Ok(*area),
            _ => Err(Ref),
        },
        Value::SignedRefs(areas) => match areas.as_slice() {
            [area] if area.size() == 1 => area.physical().ok_or(Ref),
            _ => Err(Ref),
        },
        Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => Err(*error),
        _ => Err(Ref),
    }
}
