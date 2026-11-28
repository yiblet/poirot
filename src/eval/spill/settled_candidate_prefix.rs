use super::sheet_candidates::SheetCandidates;
use crate::eval::coordinates::CellSet;

/// The leading candidates already settled in one sheet's current ordering.
#[derive(Clone, Copy)]
pub(super) struct SettledCandidatePrefix {
    sheet: usize,
    revision: u64,
    end: usize,
    settled_count: usize,
}

impl SettledCandidatePrefix {
    /// Extend the proof across newly settled candidates in source order.
    pub(super) fn advance(
        previous: Option<Self>,
        sheet: usize,
        discovery: &SheetCandidates,
        settled: &CellSet,
    ) -> Self {
        let previous = Self::matching(previous, sheet, discovery);
        let settled_count = settled.len();
        if let Some(previous) = previous
            && previous.settled_count == settled_count
        {
            return previous;
        }
        let start = previous.map_or(0, |prefix| prefix.end);
        let additional = discovery
            .candidates
            .get(start..)
            .unwrap_or_default()
            .iter()
            .take_while(|candidate| settled.contains(&(sheet, candidate.anchor)))
            .count();
        Self {
            sheet,
            revision: discovery.order_revision,
            end: start.saturating_add(additional),
            settled_count,
        }
    }

    /// Read the proof only while its sheet and candidate ordering still match.
    pub(super) fn end(prefix: Option<Self>, sheet: usize, discovery: &SheetCandidates) -> usize {
        Self::matching(prefix, sheet, discovery).map_or(0, |prefix| prefix.end)
    }

    fn matching(prefix: Option<Self>, sheet: usize, discovery: &SheetCandidates) -> Option<Self> {
        prefix.filter(|prefix| prefix.sheet == sheet && prefix.revision == discovery.order_revision)
    }
}
