/// Invalid unit range, one-based start, or encoded byte length in a text operation.
/// Malformed UTF-16 is allowed in [`ExcelText`](crate::scalar::ExcelText); converting it to UTF-8 can fail separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TextUnitsError {
    /// Requested UTF-16 unit interval lies outside the stored text.
    #[error("requested utf-16 unit interval lies outside the stored text")]
    InvalidRange,
    /// Starting UTF-16 unit position is outside the text.
    #[error("starting utf-16 unit position is outside the text")]
    InvalidStart,
    /// UTF-16 byte input contains an incomplete code unit.
    #[error("utf-16 byte input contains an incomplete code unit")]
    OddByteLength,
}
