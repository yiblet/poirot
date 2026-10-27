//! Cell results retained across calls and their retention accounting.

use crate::eval::coordinates::CellMemo;
use crate::eval::limits::MemoRetentionUsage;
use crate::model::CellAddress;

/// Memoized cell results, the admissions of the current call and the retained
/// memo size checked against retention limits.
#[derive(Default)]
pub(in crate::eval) struct ResultMemo {
    /// Evaluated cell results keyed by target.
    pub(in crate::eval) cache: CellMemo,
    /// Cells admitted during the current call, tracked only by direct workspaces.
    pub(in crate::eval) admitted: Option<Vec<(usize, CellAddress)>>,
    /// Retained memo entries and bytes counted against retention limits.
    pub(in crate::eval) usage: MemoRetentionUsage,
}
