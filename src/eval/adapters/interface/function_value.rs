use crate::eval::values::Value;
use crate::model::CellValue;

/// An adapter result may retain references or arrays until the caller consumes it.
#[derive(Clone)]
pub struct FunctionValue(
    /// Unprojected engine value channel.
    pub(in crate::eval) Value,
);

impl FunctionValue {
    /// Whether this value represents an omitted argument.
    pub fn is_missing(&self) -> bool {
        matches!(self.0, Value::Missing)
    }

    /// Wrap a scalar result without reference or array projection.
    pub fn scalar(value: CellValue) -> Self {
        Self(Value::Scalar(value))
    }
}
