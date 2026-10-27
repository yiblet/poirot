/// Case-mapping asset or context gaps reported without substituting Rust Unicode rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JavaCaseError {
    /// Pinned mapping asset failed integrity or layout validation.
    #[error("pinned mapping asset failed integrity or layout validation")]
    InvalidMappingAsset,
}
