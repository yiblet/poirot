//! Values and syntax provenance carried by the deferred host.
use super::{CellValue, Value};

/// Origin of a projected scalar, used by family-specific coercion rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum OperandOrigin {
    /// Direct scalar expression.
    Direct,
    /// Member of a materialized array.
    ArrayElement,
    /// Value read from a workbook reference.
    ReferencedCell,
}
/// Value channel before scalar coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum OperandKind {
    /// Omitted syntactic argument.
    Missing,
    /// Scalar value.
    Scalar,
    /// Materialized array.
    Array,
    /// Deferred workbook reference.
    Reference,
}
/// Explicit treatment of omitted syntax during scalar selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum MissingPolicy {
    /// Project omitted syntax as blank while retaining its missing flag.
    Blank,
    /// Test policy projecting omitted syntax as numeric zero.
    #[cfg(test)]
    Zero,
    /// Test policy rejecting omitted syntax.
    #[cfg(test)]
    Reject,
}
/// Numeric coercion contract used by the deferred host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum NumericPolicy {
    /// Pinned Apache POI conversion behavior.
    PinnedPoi,
}
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

/// Selected scalar together with its coercion provenance and omission flag.
pub(in crate::eval) struct ScalarOperand {
    /// Selected scalar value.
    pub(in crate::eval) value: CellValue,
    /// Direct, array, or reference origin.
    pub(in crate::eval) origin: OperandOrigin,
    /// Whether source syntax omitted this scalar.
    pub(in crate::eval) missing: bool,
}
/// Operand shape and channel inspected without traversing member cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) struct OperandDescription {
    /// Number of rows.
    pub(in crate::eval) rows: usize,
    /// Number of columns.
    pub(in crate::eval) columns: usize,
    /// Missing, scalar, array, or reference channel.
    pub(in crate::eval) kind: OperandKind,
    /// Whether the source is an area reference.
    pub(in crate::eval) is_area: bool,
    /// Number of referenced sheets.
    pub(in crate::eval) sheets: usize,
    /// Whether the original argument was omitted.
    pub(in crate::eval) missing: bool,
}
