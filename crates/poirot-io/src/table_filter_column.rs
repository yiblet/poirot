use crate::TableFilterValues;

/// Stored filter controls and criteria for one AutoFilter column.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableFilterColumn {
    /// Zero-based index inside the AutoFilter rectangle.
    pub index: u32,
    /// Explicit hidden-button flag.
    pub hidden_button: Option<bool>,
    /// Explicit show-button flag.
    pub show_button: Option<bool>,
    /// The modeled choice of value and blank criteria, when present.
    pub filters: Option<TableFilterValues>,
}
