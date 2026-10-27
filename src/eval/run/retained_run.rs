use super::run_report::RunReport;
use crate::eval::coordinates::CellMemo;
use crate::eval::limits::MemoRetentionUsage;

/// Owned results of one run. This is the Session memo moved at completion,
/// never a second cache and never a source of defaults for another run.
pub struct RetainedRun<T> {
    /// Completion or abort report from the finished run.
    pub report: RunReport<T>,
    /// Storage charged for the retained memo.
    pub memo_usage: MemoRetentionUsage,
    /// Memo moved from the run without duplicating its values.
    pub cells: CellMemo,
}
