/// Worksheet publication of final array results, independent of builtin semantics.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ArrayResultPolicy {
    /// Reduce final arrays to a scalar using intersection behavior.
    ScalarIntersection,
    /// Prepare intersecting possible spill rectangles and their necessary competitors.
    /// Publish arrays and area-reference roots atomically with ownership and blocker checks.
    /// Accepted one-cell arrays retain a footprint for postfix spill references (`#`).
    /// Actual overlaps always favor the top-left anchor (row, then column).
    /// Dependency components settle privately; operational failures poison the evaluation.
    #[default]
    DynamicSpill,
}
