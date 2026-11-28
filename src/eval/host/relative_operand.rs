use crate::eval::values::{Area, SignedArea};

/// A deferred operand bound for repeated reads relative to the same origin.
#[derive(Clone, Copy)]
pub(in crate::eval) enum RelativeOperand {
    /// Reference whose projected offsets may extend past the authored endpoint.
    Physical(Area),
    /// Reference with logical offsets that may project off-grid.
    Signed(SignedArea),
    /// Array, sheet span or scalar that retains its original coordinate bounds.
    Indexed(usize),
}
