/// Syntactic/runtime origin of an evaluated argument before value projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentOrigin {
    /// Omitted syntactic operand.
    Missing,
    /// Literal expression.
    Literal,
    /// Reference expression.
    Reference,
    /// Expression requiring computation.
    Computed,
}
