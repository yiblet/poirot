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
