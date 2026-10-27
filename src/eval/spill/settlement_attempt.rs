use super::attempted_footprint::AttemptedFootprint;
use super::completion::Completion;
use crate::eval::coordinates::{CellMap, CellSet};
use crate::eval::limits::MemoRetentionUsage;
use crate::model::CellAddress;

/// Rollback state for one connected group of competing array producers.
/// It tracks provisional admissions and delayed notifications so retries can discard
/// this attempt without removing results settled earlier in the run.
pub(super) struct SettlementAttempt {
    pub(super) selected: std::collections::BTreeSet<(usize, CellAddress)>,
    pub(super) blocked: CellSet,
    pub(super) footprints: CellMap<AttemptedFootprint>,
    pub(super) admitted: CellSet,
    pub(super) consumers: CellSet,
    pub(super) random_touched: CellSet,
    pub(super) memo_usage: MemoRetentionUsage,
    pub(super) journal_len: usize,
    pub(super) admitted_len: usize,
    pub(super) notifications: Vec<Completion>,
    pub(super) non_repeatable: bool,
}
