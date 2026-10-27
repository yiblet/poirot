use super::expansion::Expansion;
use super::producer_candidate::ProducerCandidate;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError};
use crate::eval::values::Area;
use crate::model::CellAddress;

/// A sheet's possible array producers and the source regions already inspected for them.
/// The inspected prefixes prove discovery is complete for a demand; candidate
/// rectangles only bound possible footprints and do not establish ownership.
#[derive(Default)]
pub(in crate::eval) struct SheetCandidates {
    pub(super) candidates: Vec<ProducerCandidate>,
    /// Monotone maximum possible ending row through each ordered candidate.
    pub(super) ending_rows: Vec<u32>,
    /// Maximal prefix endpoints keyed by column; rows strictly decrease.
    pub(super) inspected: std::collections::BTreeMap<u32, u32>,
    /// Saved children without declared geometry cannot supply finite bounds.
    pub(super) legacy: std::sync::Arc<[CellAddress]>,
}

impl SheetCandidates {
    /// Whether every authored coordinate in this origin prefix was inspected successfully.
    pub(super) fn covers(&self, last: CellAddress) -> bool {
        self.inspected
            // SAFETY: With no upper bound, this included lower bound cannot be reversed.
            .range(last.col..)
            .next()
            .is_some_and(|(_, row)| *row >= last.row)
    }

    /// Bound candidate tests by possible ending rows and source-priority starts.
    pub(super) fn possible_in(
        &self,
        area: Area,
        before: Option<CellAddress>,
    ) -> &[ProducerCandidate] {
        let end = match before {
            Some(anchor) => self.candidates.partition_point(|c| c.anchor < anchor),
            None => self.candidates.partition_point(|c| c.anchor.row <= area.r1),
        };
        let start = self
            .ending_rows
            .partition_point(|row| *row < area.r0)
            .min(end);
        self.candidates.get(start..end).unwrap_or_default()
    }

    /// Stage only uncovered bands and dominated endpoints, without changing the proof.
    pub(super) fn expansion(
        &self,
        sheet: usize,
        last: CellAddress,
        budget: &mut BudgetMeter,
        consumer: Option<EvaluationTarget>,
    ) -> EvalResult<Option<Expansion>> {
        // SAFETY: An unbounded upper endpoint cannot precede this included lower endpoint.
        let boundary = self.inspected.range(last.col..).next();
        let covered_row = boundary.map(|(_, row)| *row);
        let replaces_column = boundary.is_some_and(|(column, _)| *column == last.col);
        if covered_row.is_some_and(|row| row >= last.row) {
            return Ok(None);
        }
        let mut expansion = Expansion {
            strips: Vec::new(),
            removed_columns: Vec::new(),
            frontier_size: 0,
            candidate_count: self.candidates.len(),
            legacy: Some(self.legacy.clone()),
        };
        let mut row = covered_row.map_or(0, |row| row.saturating_add(1));
        // Rows increase in this reverse column traversal. Every visited endpoint
        // except at most the final boundary is dominated by the new prefix.
        // SAFETY: With no lower bound, this included upper bound cannot be reversed.
        for (&column, &end_row) in self.inspected.range(..=last.col).rev() {
            if end_row >= row && column < last.col {
                expansion.strip(
                    sheet,
                    CellAddress {
                        row,
                        col: column.saturating_add(1),
                    },
                    CellAddress {
                        row: end_row.min(last.row),
                        col: last.col,
                    },
                    budget,
                    consumer,
                )?;
            }
            if end_row <= last.row && column != last.col {
                budget.check_allocation(expansion.removed_columns.len().saturating_add(1))?;
                expansion.removed_columns.try_reserve(1).map_err(|reservation_source| {
                    EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        target: consumer,
                        reason: (crate::eval::limits::EvaluationReasonError::SpillFrontierReplacementAllocationFailed).with_reservation_source(reservation_source),
                    }
                })?;
                // SAFETY: The successful reservation provides one dominated endpoint slot.
                expansion.removed_columns.push(column);
            }
            row = row.max(end_row.min(last.row).saturating_add(1));
            if row > last.row {
                break;
            }
        }
        if row <= last.row {
            expansion.strip(sheet, CellAddress { row, col: 0 }, last, budget, consumer)?;
        }
        expansion.frontier_size = self
            .inspected
            .len()
            .saturating_sub(expansion.removed_columns.len())
            .saturating_add(usize::from(!replaces_column));
        Ok(Some(expansion))
    }
}
