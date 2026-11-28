use super::sparse_member::SparseMember;
use crate::eval::inputs::RunInputs;
use crate::eval::values::Area;
use crate::model::Workbook;

/// Owned sparse members and the immutable source rectangle they can cover again.
#[derive(Default)]
pub(in crate::eval) struct SparseBatch<'a> {
    /// Occupied coordinates in row-major order.
    pub(super) members: Vec<SparseMember<'a>>,
    /// Authored-only bounds and source identity, when reuse is admitted.
    pub(super) source: Option<(Area, &'a Workbook, Option<&'a RunInputs>)>,
}

impl<'a> SparseBatch<'a> {
    /// Borrow captured members while evaluating the current operand.
    pub(in crate::eval) fn members(&self) -> &[SparseMember<'a>] {
        &self.members
    }

    /// Transfer existing ownership to callers that do not return traversal storage.
    pub(in crate::eval) fn into_members(self) -> Vec<SparseMember<'a>> {
        self.members
    }

    /// Keep overlapping rows from a forward window and return any missing tail.
    pub(super) fn missing_tail(
        &mut self,
        area: Area,
        workbook: &'a Workbook,
        inputs: Option<&'a RunInputs>,
        can_reuse: bool,
    ) -> Option<Area> {
        if can_reuse
            && let Some((prior, prior_workbook, prior_inputs)) = self.source
            && area.sheet == prior.sheet
            && area.c0 == prior.c0
            && area.c1 == prior.c1
            && prior.r0 <= area.r0
            && area.r0 <= prior.r1
            && area.r0 <= area.r1
            && std::ptr::eq(prior_workbook, workbook)
            && match (prior_inputs, inputs) {
                (None, None) => true,
                (Some(prior), Some(current)) => std::ptr::eq(prior, current),
                _ => false,
            }
        {
            if area.r0 > prior.r0 {
                let discarded = self
                    .members
                    .partition_point(|member| member.address.row < area.r0);
                // SAFETY: partition_point returns an index at most equal to the vector length.
                drop(self.members.drain(..discarded));
            }
            if area.r1 < prior.r1 {
                let count = self
                    .members
                    .partition_point(|member| member.address.row <= area.r1);
                self.members.truncate(count);
            }
            if area.r1 <= prior.r1 {
                return None;
            }
            return Some(Area {
                // SAFETY: prior.r1 is strictly lower than area.r1.
                r0: prior.r1 + 1,
                ..area
            });
        }
        self.members.clear();
        Some(area)
    }
}
