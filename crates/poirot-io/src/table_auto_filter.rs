use crate::{CellAddress, TableFilterColumn};

/// The presence, range and supported definitions of a table's AutoFilter.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableAutoFilter {
    /// Stored filter rectangle, when explicitly supplied.
    pub range: Option<(CellAddress, CellAddress)>,
    /// Filter criteria and control flags by zero-based column offset.
    pub columns: Vec<TableFilterColumn>,
}
