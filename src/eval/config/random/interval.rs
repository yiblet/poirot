/// Inclusive integer interval whose members are exactly representable in binary64.
#[derive(Clone, Copy)]
pub(in crate::eval) struct Interval {
    pub(super) bottom: i64,
    pub(super) width: u64,
}

impl Interval {
    /// Validate integral ordered bounds within the supported exact-number envelope.
    pub fn new(bottom: f64, top: f64) -> Option<Self> {
        // Every integer in this envelope is exactly representable in binary64.
        const LIMIT: f64 = 9007199254740992.0;
        if !bottom.is_finite()
            || !top.is_finite()
            || bottom.fract() != 0.0
            || top.fract() != 0.0
            || bottom < -LIMIT
            || top > LIMIT
            || bottom > top
        {
            return None;
        }
        let bottom = bottom as i64;
        Some(Self {
            bottom,
            width: (top as i64 - bottom) as u64 + 1,
        })
    }
}
