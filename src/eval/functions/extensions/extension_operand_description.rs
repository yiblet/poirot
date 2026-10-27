use super::extension_operand_kind::ExtensionOperandKind;

/// Argument kind and rectangular dimensions without traversing referenced cells.
/// Describing an argument can evaluate its expression to determine its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtensionOperandDescription {
    /// Operand channel before scalar coercion.
    pub kind: ExtensionOperandKind,
    /// Number of rows in the operand shape.
    pub rows: usize,
    /// Number of columns in the operand shape.
    pub columns: usize,
}
