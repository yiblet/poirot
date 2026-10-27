/// Argument channel before scalar coercion: omitted syntax, scalar, array, or reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionOperandKind {
    /// Omitted syntactic argument.
    Missing,
    /// Single scalar value, including spreadsheet errors.
    Scalar,
    /// Materialized rectangular array.
    Array,
    /// Deferred workbook reference.
    Reference,
}
