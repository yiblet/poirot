use super::source_identity::SourceIdentity;
use std::hash::{Hash, Hasher};

/// In-process lookup hint. Exact metadata equality is always required for validity;
/// this digest is not a persistent identity or collision-proof authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SourceFingerprint(
    /// Process-local digest; exact source metadata must still be compared.
    pub u64,
);

pub(super) fn fingerprint(identity: &SourceIdentity) -> SourceFingerprint {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    identity.hash(&mut hash);
    SourceFingerprint(hash.finish())
}
