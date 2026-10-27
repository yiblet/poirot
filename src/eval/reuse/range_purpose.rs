/// Which dependency rectangles an index resolves.
pub(super) enum RangePurpose {
    /// Values consumed by formulas, including compressed point reads.
    Values,
    /// Potential ownership or future-growth guards.
    SpillGeometry,
}
