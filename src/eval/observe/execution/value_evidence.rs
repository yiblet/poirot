use super::observed_value::ObservedValue;
use crate::eval::values::Value;

/// Borrow scalar evidence or summarize the shape of a nonscalar value.
pub(in crate::eval) fn observed_value(value: &Value) -> ObservedValue<'_> {
    match value {
        Value::Scalar(value) | Value::SelectedScalar(value, _) => ObservedValue::Scalar(value),
        Value::Missing => ObservedValue::Missing,
        Value::Array(array) => ObservedValue::Array {
            rows: array.values.len(),
            values: array.values.iter().map(Vec::len).sum(),
        },
        Value::Refs(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: false,
        },
        Value::SignedRefs(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: false,
        },
        Value::SheetSpan(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: true,
        },
    }
}
