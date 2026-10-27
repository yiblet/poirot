use crate::eval::reuse::{Cache, Reads};
use crate::eval::spill::CandidateIndex;
use std::cell::RefCell;

/// Overlay-owned handles that let a run reuse and record incremental evidence.
#[derive(Default)]
pub(in crate::eval) struct ReuseHandles<'a> {
    /// Indexed spill producers shared across runs of one overlay.
    pub(in crate::eval) spill_candidates: Option<&'a RefCell<CandidateIndex>>,
    /// Dependency evidence retained by an overlay across edits.
    pub(in crate::eval) overlay_cache: Option<&'a RefCell<Cache>>,
    /// Reads recorded for the overlay's dependency tracking.
    pub(in crate::eval) overlay_reads: Option<Reads>,
}
