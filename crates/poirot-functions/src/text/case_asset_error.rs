/// Integrity/layout failure while reading the pinned full-case mapping tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid Unicode case mapping asset")]
pub(super) struct CaseAssetError;
