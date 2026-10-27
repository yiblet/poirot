//! Dynamic-array discovery, settlement and publication.
//!
//! `discovery::Discovery` reads authored cells and shape bounds without executing
//! formulas. `settlement::Settlement` owns component selection, retries and rollback
//! through the `Execution` port, which `SpillAccess` implements over a borrowed
//! session. `admission::Admission` publishes memoized results using only storage
//! and resource capabilities. Session-facing behavior runs through executors that
//! borrow the session: `SpillDemand` settles demanded reads and spill references,
//! `ProducerExecution` computes and admits private producers, `SpillPublisher`
//! stages and admits finished cell results, and `SpillQuery` answers read-only
//! ownership questions.
//!
//! Depends on: session (Session state, cell evaluation, reads and result projection:
//! producers execute formulas and formulas read spills), compile (source preparation
//! and shape bounds), config (array result and producer policies), functions (extension
//! cacheability), coordinates (targets, coordinate tables and memo), limits (budgets),
//! values (value channels and reference geometry), reuse (retained results, read
//! recording and spill roles), ports (completion and read events) and inputs (effective
//! workbook).

/// Storage-only admission of computed results.
mod admission;
/// Physical anchors of spill references.
mod anchor;
/// A producer's actual attempted array footprint used to decide spill conflicts.
mod attempted_footprint;
/// Per-sheet candidate discovery proofs and their restoration.
mod candidate_index;
/// Owned cell completion withheld until actual ownership has settled.
mod completion;
/// A completed worksheet value and its optional unpublished rectangular footprint.
mod computed_cell;
/// Source-only discovery of possible array producers.
mod discovery;
/// The settlement engine's execution port.
mod execution;
/// Stages the uninspected source strips needed to extend a sheet's discovery proof.
mod expansion;
/// A computed formula withheld from ordinary caches until ownership settles.
mod held;
/// An authored formula whose inferred shape could overlap a requested rectangle.
mod producer_candidate;
/// Private producer computation and admission during settlement.
mod producer_execution;
/// Records a producer's blank and sparse reads that may overlap its eventual array output.
mod producer_read_evidence;
/// Settlement queueing of potential spill owners from inspected geometry.
mod producer_selection;
/// Read-only evidence of settled spill ownership for incremental cache publication.
mod publication;
/// The observation responsible for an unpublished-footprint dependency.
mod self_read;
/// Marks a requested rectangle whose possible producers have already been discovered and settled.
mod settled_area;
/// Demand selection, retry, rollback and settlement.
mod settlement;
/// Rollback state for one connected group of competing array producers.
mod settlement_attempt;
/// One lazily allocated component workspace, active only during private settlement.
mod settlement_slot;
/// A sheet's possible array producers and the source regions already inspected for them.
mod sheet_candidates;
/// The settlement engine's execution port over a borrowed session.
mod spill_access;
/// Demand-driven settlement for reads and spill references.
mod spill_demand;
/// Tracks array discovery, provisional producer results, and settled spill ownership for a run.
mod spill_preparation;
/// Publication of finished cell results and their settlement effects.
mod spill_publisher;
/// Read-only spill ownership queries.
mod spill_query;
/// Unit tests for spill discovery, settlement, admission and publication.
#[cfg(test)]
mod tests;

pub(in crate::eval) use anchor::source_anchor;
pub(super) use candidate_index::CandidateIndex;
pub(in crate::eval) use candidate_index::{inspected_prefixes, restore_candidates};
pub(super) use computed_cell::ComputedCell;
pub(in crate::eval) use settlement::unsupported;
pub(in crate::eval) use spill_demand::SpillDemand;
pub(super) use spill_preparation::SpillPreparation;
pub(in crate::eval) use spill_publisher::SpillPublisher;
pub(in crate::eval) use spill_query::SpillQuery;
