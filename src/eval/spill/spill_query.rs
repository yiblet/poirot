//! Read-only questions about a session's spill discovery and settlement state.
use super::settled_area::SettledArea;
use super::sheet_candidates::SheetCandidates;
use crate::eval::limits::{EvalResult, EvaluationError};
use crate::eval::session::Session;
use crate::eval::values::Area;
use crate::model::CellAddress;

/// Answers spill ownership questions from a shared session borrow.
///
/// Readers that hold only `&Session`, including the settlement port itself,
/// use this view; it never executes formulas or mutates spill state.
pub(in crate::eval) struct SpillQuery<'s, 'a> {
    session: &'s Session<'a>,
}

impl<'s, 'a> SpillQuery<'s, 'a> {
    /// Borrow the session for read-only spill queries.
    pub(in crate::eval) fn new(session: &'s Session<'a>) -> Self {
        Self { session }
    }

    /// Borrow the authoritative index only for this nonrecursive inspection.
    pub(super) fn inspect<R>(
        &self,
        sheet: usize,
        inspect: impl FnOnce(&SheetCandidates) -> R,
    ) -> EvalResult<R> {
        if let Some(shared) = self.session.reuse.spill_candidates {
            let index = shared
                .try_borrow()
                .map_err(|_| EvaluationError::StalePlan)?;
            return index
                .get(&sheet)
                .map(inspect)
                .ok_or(EvaluationError::StalePlan);
        }
        self.session
            .state
            .runtime
            .preparation
            .candidates
            .get(&sheet)
            .map(inspect)
            .ok_or(EvaluationError::StalePlan)
    }

    /// Whether all possible owners of this physical area have settled.
    pub(in crate::eval) fn ready(&self, area: Area) -> bool {
        let preparation = &self.session.state.runtime.preparation;
        preparation.failure.is_none()
            && preparation.settlement.active().is_none()
            && (preparation.ready.is_some_and(|SettledArea(ready)| {
                ready.sheet == area.sheet
                    && ready.r0 <= area.r0
                    && area.r1 <= ready.r1
                    && ready.c0 <= area.c0
                    && area.c1 <= ready.c1
            }) || self
                .inspect(area.sheet, |discovery| {
                    discovery.covers(CellAddress {
                        row: area.r1,
                        col: area.c1,
                    }) && discovery.possible_in(area, None).iter().all(|c| {
                        !c.intersects(area) || preparation.settled.contains(&(area.sheet, c.anchor))
                    })
                })
                .unwrap_or(false))
    }

    /// Whether reads currently participate in private ownership settlement.
    pub(in crate::eval) fn settling(&self) -> bool {
        self.session
            .state
            .runtime
            .preparation
            .settlement
            .active()
            .is_some()
    }

    /// Read validated candidate membership without another compiler lookup.
    pub(in crate::eval) fn candidate(&self, sheet: usize, at: CellAddress) -> bool {
        self.inspect(sheet, |p| {
            p.candidates.binary_search_by_key(&at, |c| c.anchor).is_ok()
        })
        .unwrap_or(false)
    }
}
