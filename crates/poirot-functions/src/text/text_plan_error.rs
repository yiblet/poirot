/// Selection/reservation failure before copying UTF-16 text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextPlanError {
    /// Invalid text-function position or count.
    #[error("invalid text-function position or count")]
    Value,
    /// Output-size or reservation arithmetic overflowed.
    #[error("output-size or reservation arithmetic overflowed")]
    SizeOverflow,
}
