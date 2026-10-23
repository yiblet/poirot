//! Fallible storage admission separates representation overflow from allocator refusal.
use crate::{CapabilityId, EvaluationFailure, EvaluationTarget, ResourceFailureKind};

/// Reserve a vector's additional slots after checking Rust's byte-capacity representation.
pub(crate) fn reserve_exact<T>(
    values: &mut Vec<T>,
    additional: usize,
    source: Option<EvaluationTarget>,
    capability: CapabilityId,
    operation: &'static str,
) -> Result<(), EvaluationFailure> {
    let representable = values
        .len()
        .checked_add(additional)
        .and_then(|len| len.checked_mul(std::mem::size_of::<T>()))
        .is_some_and(|bytes| bytes <= isize::MAX as usize);
    if !representable {
        return Err(EvaluationFailure::Resource {
            kind: ResourceFailureKind::SizeOverflow,
            capability,
            source,
            detail: operation.into(),
        });
    }
    values
        .try_reserve_exact(additional)
        .map_err(|_| EvaluationFailure::Resource {
            kind: ResourceFailureKind::Allocation,
            capability,
            source,
            detail: operation.into(),
        })
}
