/// AreaEvalBase computes dimensions with Java int arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum SourceDimension {
    /// Positive dimension usable for iteration.
    Positive(u32),
    /// Java arithmetic produced zero.
    Zero,
    /// Java signed arithmetic overflowed to a negative value.
    NegativeOverflow(i32),
}

impl SourceDimension {
    /// Measure one axis with Java int wrapping arithmetic.
    pub(super) fn axis(first: i32, last: i32) -> Self {
        match last.wrapping_sub(first).wrapping_add(1) {
            0 => Self::Zero,
            n if n > 0 => Self::Positive(n as u32),
            n => Self::NegativeOverflow(n),
        }
    }

    /// Return the source-compatible signed dimension.
    pub(in crate::eval) fn value(self) -> i32 {
        match self {
            Self::Positive(n) => n as i32,
            Self::Zero => 0,
            Self::NegativeOverflow(n) => n,
        }
    }

    /// Count iterations, treating nonpositive dimensions as empty.
    pub(super) fn iterations(self) -> u64 {
        self.value().max(0) as u64
    }
}
