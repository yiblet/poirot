use super::selection::compare;
use crate::model::{CellError, CellValue};
use poirot_functions::matrix::{Axis, Budget, Group, Matrix, Partition};
use poirot_functions::statistics::PivotMembers;

/// Keeps each pivot group paired with source-ordered record membership.
/// This lets row and column groups intersect without relying on their display order.
pub(super) struct PivotAxis {
    pub(super) keys: Matrix<CellValue>,
    pub(super) partition: Partition,
    pub(super) groups: Vec<Group>,
    pub(super) members: Vec<PivotMembers>,
}

impl PivotAxis {
    /// Build group memberships with the shared pure kernels and the supplied budget.
    pub(super) fn new(
        keys: Matrix<CellValue>,
        budget: Budget,
    ) -> poirot_functions::matrix::Result<Self> {
        let partition = poirot_functions::matrix::partition(&keys, Axis::Rows, compare, budget)?;
        let groups = poirot_functions::matrix::hierarchy(&keys, &partition, compare, budget)?;
        let mut members = Vec::new();
        members
            .try_reserve_exact(groups.len())
            .map_err(|_| poirot_functions::matrix::MatrixError::AllocationFailure)?;
        for group in &groups {
            let ordinals = partition.order.get(group.members.clone()).ok_or(
                poirot_functions::matrix::MatrixError::Formula(CellError::Value),
            )?;
            let selection = PivotMembers::new(ordinals, keys.rows(), budget)?;
            // SAFETY: One slot per unchanged group was reserved locally, and each group emits once.
            members.push(selection);
        }
        Ok(Self {
            keys,
            partition,
            groups,
            members,
        })
    }
}
