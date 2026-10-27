use crate::eval::values::Area;

/// A producer's actual attempted array footprint used to decide spill conflicts.
/// The saved collision flag preserves conflicts with previously settled ownership
/// while the active component retries its own producers.
#[derive(Clone, Copy)]
pub(super) struct AttemptedFootprint {
    pub(super) area: Area,
    pub(super) settled_collision: bool,
}
