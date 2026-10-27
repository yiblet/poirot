use super::operand_kind::OperandKind;

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
