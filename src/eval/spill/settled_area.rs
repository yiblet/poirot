use crate::eval::values::Area;

/// Marks a requested rectangle whose possible producers have already been discovered and settled.
/// Repeated reads can reuse this proof while the evaluation's source and ownership stay unchanged.
#[derive(Clone, Copy)]
pub(super) struct SettledArea(pub(super) Area);
