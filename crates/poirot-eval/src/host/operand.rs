//! Values and syntax provenance carried by the deferred host.
use super::{CellValue, Value};

/// Origin of a projected scalar, used by family-specific coercion rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperandOrigin {
    /// Direct scalar expression.
    Direct,
    /// Member of a materialized array.
    ArrayElement,
    /// Value read from a workbook reference.
    ReferencedCell,
}
/// Value channel before scalar coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OperandKind {
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
pub(crate) enum MissingPolicy {
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
pub(crate) enum NumericPolicy {
    /// Test path preserving legacy conversion behavior.
    #[cfg(test)]
    LegacyCompatible,
    /// Pinned Apache POI conversion behavior.
    PinnedPoi,
}
/// Missing is syntax metadata, never inferred from a blank worksheet cell.
pub(crate) struct HostOperand {
    /// Unprojected engine value.
    pub(super) value: Value,
    /// Whether the original operand was omitted.
    pub(super) missing: bool,
    /// Per-element omitted flags for literal arrays.
    pub(super) array_missing: Vec<Vec<bool>>,
}
impl HostOperand {
    /// Inspect the value channel without dereferencing it.
    pub(crate) fn kind(&self) -> OperandKind {
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
    /// Wrap an explicit scalar with no omitted-argument provenance.
    pub(crate) fn from_scalar(value: CellValue) -> Self {
        Self {
            value: value.into(),
            missing: false,
            array_missing: Vec::new(),
        }
    }
    /// Restore the missing marker or return the unprojected engine value.
    pub(crate) fn into_value(self) -> Value {
        if self.missing {
            Value::Missing
        } else {
            self.value
        }
    }
}

/// Selected scalar together with its coercion provenance and omission flag.
pub(crate) struct ScalarOperand {
    /// Selected scalar value.
    pub(crate) value: CellValue,
    /// Direct, array, or reference origin.
    pub(crate) origin: OperandOrigin,
    /// Whether source syntax omitted this scalar.
    pub(crate) missing: bool,
}
/// Operand shape and channel inspected without traversing member cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OperandDescription {
    /// Number of rows.
    pub(crate) rows: usize,
    /// Number of columns.
    pub(crate) columns: usize,
    /// Missing, scalar, array, or reference channel.
    pub(crate) kind: OperandKind,
    /// Whether the source is an area reference.
    pub(crate) is_area: bool,
    /// Number of referenced sheets.
    pub(crate) sheets: usize,
    /// Whether the original argument was omitted.
    pub(crate) missing: bool,
}
