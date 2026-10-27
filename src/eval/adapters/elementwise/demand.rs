/// Demand timing independent of a role's scalar, collection, or broadcast shape.
pub(in crate::eval::adapters) trait Demand {
    /// Whether signature admission may describe this operand's geometry.
    const EAGER: bool;
}

/// Demand marker that permits signature preparation to inspect an argument before output traversal.
pub(in crate::eval::adapters) struct Eager;

/// Demand marker that postpones an argument's geometry and value until a selector chooses it.
pub(in crate::eval::adapters) struct Lazy;

impl Demand for Eager {
    const EAGER: bool = true;
}

impl Demand for Lazy {
    const EAGER: bool = false;
}
