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
