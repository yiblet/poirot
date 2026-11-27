/// A table's style selection and presentation flags.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableStyleInfo {
    /// Selected built-in or custom style name, as stored in the table part.
    pub name: Option<String>,
    /// Show formatting for the first column.
    pub show_first_column: Option<bool>,
    /// Show formatting for the last column.
    pub show_last_column: Option<bool>,
    /// Show alternating row formatting.
    pub show_row_stripes: Option<bool>,
    /// Show alternating column formatting.
    pub show_column_stripes: Option<bool>,
}
