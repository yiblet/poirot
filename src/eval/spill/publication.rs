use super::spill_preparation::SpillPreparation;
use crate::eval::reuse::SpillRole;
use crate::model::CellAddress;

/// Read-only evidence of settled spill ownership for incremental cache publication.
/// SpillPreparation exposes this view only when no settlement is active or failed,
/// preventing provisional children from entering retained results.
pub(in crate::eval) struct Publication<'a> {
    pub(super) preparation: &'a SpillPreparation,
}

impl Publication<'_> {
    /// Count settled generated coordinates without visiting their values.
    pub(in crate::eval) fn generated_count(&self) -> usize {
        self.preparation.roles.len()
    }

    /// Borrow settled generated children with their producers.
    pub(in crate::eval) fn children(
        &self,
    ) -> impl Iterator<Item = ((usize, CellAddress), CellAddress)> + '_ {
        self.preparation
            .roles
            .iter()
            .filter_map(|(&key, role)| match role {
                SpillRole::Child { anchor } => Some((key, *anchor)),
                _ => None,
            })
    }

    /// Read a settled producer's ending coordinate.
    pub(in crate::eval) fn extent(&self, key: (usize, CellAddress)) -> Option<CellAddress> {
        self.role(key).last()
    }

    /// Newly admitted or restored coordinates awaiting a successful cache commit.
    pub(in crate::eval) fn pending_cells(&self) -> &[(usize, CellAddress)] {
        &self.preparation.journal
    }

    /// Read the settled role belonging to a coordinate in this evaluation.
    pub(in crate::eval) fn role(&self, key: (usize, CellAddress)) -> SpillRole {
        self.preparation.admitted_role(key)
    }

    /// Whether a coordinate belongs to the settled generated footprint.
    pub(in crate::eval) fn contains(&self, key: (usize, CellAddress)) -> bool {
        self.preparation.has_admitted_role(key)
    }
}
