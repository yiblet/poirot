/// A text operation that cannot be represented under the selected rules.
/// These failures are distinct from spreadsheet `#VALUE!` errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextSemanticsError {
    /// Selected UTF-16 units cannot be returned as borrowed UTF-8.
    #[error("selected utf-16 units cannot be returned as borrowed utf-8")]
    UnpairedSurrogateResult,
    /// Requested byte boundary splits a UTF-8 character.
    #[error("requested byte boundary splits a utf-8 character")]
    NonScalarByteBoundary,
    /// Text-size arithmetic overflowed before allocation.
    #[error("text-size arithmetic overflowed before allocation")]
    SizeOverflow,
}
