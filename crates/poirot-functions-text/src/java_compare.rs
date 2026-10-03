//! Standard Unicode simple folding for the historical lookup API.
//! The compatibility facade keeps call signatures while using permissive tables.
use std::cmp::Ordering;
#[allow(dead_code)]
#[path = "history_fold.rs"]
mod folding;
/// Version of the standard Unicode comparison policy.
pub const POLICY_VERSION: &str = "unicode16-simple-fold-v1";
/// Failure retained for the historical fallible comparison interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// Historical asset validation failure; embedded folding has no such failure.
    InvalidMappingAsset,
}
/// Admit the immutable standard folding tables.
pub fn admit() -> Result<(), Unsupported> {
    Ok(())
}
/// Preserve the historical conservative reservation for lookup admission.
pub fn admission_work_bound() -> u64 {
    6000
}
/// Reserve comparison work based on the input unit counts.
pub fn work_bound(left_units: usize, right_units: usize) -> Option<usize> {
    left_units
        .checked_add(right_units)?
        .checked_mul(64)?
        .checked_add(1)
}
/// Compare decoded UTF-16 scalars after standard simple folding.
pub fn compare<L, R>(left: L, right: R) -> Result<Ordering, Unsupported>
where
    L: Iterator<Item = u16> + Clone,
    R: Iterator<Item = u16> + Clone,
{
    Ok(folding::compare_utf16(left, right))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_folding_preserves_unpaired_units() {
        assert_eq!(
            compare("Hello".encode_utf16(), "HELLO".encode_utf16()),
            Ok(Ordering::Equal)
        );
        assert_ne!(
            compare("İ".encode_utf16(), "i".encode_utf16()),
            Ok(Ordering::Equal)
        );
        assert_eq!(
            compare([0xd800].into_iter(), [0xd800].into_iter()),
            Ok(Ordering::Equal)
        );
        assert_eq!(work_bound(usize::MAX, 1), None);
    }
}
