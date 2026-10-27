/// Caller-selected Excel language class for legacy byte-indexed text functions.
/// Non-DBCS counts UTF-16 units, while ordinary text counts surrogate pairs together.
/// This setting does not detect the host locale; DBCS operations are unsupported.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ByteTextLanguage {
    /// A default language without a double-byte character set.
    #[default]
    NonDbcs,
    /// Japanese, Chinese, or Korean DBCS semantics (not yet supported).
    Dbcs,
}
