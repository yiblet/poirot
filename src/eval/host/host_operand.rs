//! Values and syntax provenance carried by the deferred host.
use super::operand_kind::OperandKind;
use crate::eval::values::Value;

/// An evaluated argument retained before a consumer chooses scalar, array, or reference access.
/// Syntax omission is kept separately so a blank worksheet cell cannot acquire
/// omitted-argument defaults during coercion.
pub(in crate::eval) struct HostOperand {
    /// Unprojected engine value.
    pub(super) value: Value,
    /// Whether the original operand was omitted.
    pub(super) missing: bool,
    /// Per-element omitted flags for literal arrays.
    pub(super) array_missing: Vec<Vec<bool>>,
}

impl HostOperand {
    /// Wrap an engine value for traversal without syntax omission metadata.
    #[cfg(test)]
    pub(in crate::eval) fn from_value(value: Value) -> Self {
        Self {
            value,
            missing: false,
            array_missing: Vec::new(),
        }
    }

    /// Separate a resolved operand into its value and omission metadata.
    pub(in crate::eval) fn into_parts(self) -> (Value, bool, Vec<Vec<bool>>) {
        (self.value, self.missing, self.array_missing)
    }

    /// Borrow the resolved value without projecting or dereferencing it.
    pub(in crate::eval) fn value(&self) -> &Value {
        &self.value
    }

    /// Inspect the value channel without dereferencing it.
    pub(in crate::eval) fn kind(&self) -> OperandKind {
        if self.missing {
            OperandKind::Missing
        } else {
            match self.value {
                Value::Missing => OperandKind::Missing,
                Value::Scalar(_) | Value::SelectedScalar(_, _) => OperandKind::Scalar,
                Value::Array(_) => OperandKind::Array,
                Value::Refs(_) | Value::SheetSpan(_) | Value::SignedRefs(_) => {
                    OperandKind::Reference
                }
            }
        }
    }

    #[cfg(test)]
    /// Borrow literal-array omission flags for host tests.
    pub(super) fn array_missing(&self) -> &[Vec<bool>] {
        &self.array_missing
    }

    /// Restore the missing marker or return the unprojected engine value.
    pub(in crate::eval) fn into_value(self) -> Value {
        if self.missing {
            Value::Missing
        } else {
            self.value
        }
    }
}
