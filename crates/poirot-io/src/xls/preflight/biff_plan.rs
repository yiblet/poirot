//! Admitted workbook-stream measurements.
use std::collections::BTreeMap;

use crate::RowVisibility;

/// Worksheet traversals admitted by preflight, which decoding reuses.
#[derive(Debug, Clone, Default)]
pub(in crate::xls) struct BiffPlan {
    /// Checked worksheet count.
    pub(in crate::xls) sheets: usize,
    /// Explicit ROW records recovered for each sheet, with their hidden flags.
    pub(in crate::xls) rows: Vec<BTreeMap<u32, RowVisibility>>,
    /// Record index where each worksheet traversal starts; decoding reuses it.
    pub(in crate::xls) sheet_starts: Vec<usize>,
}
