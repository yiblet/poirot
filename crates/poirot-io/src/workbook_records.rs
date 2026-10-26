//! Stored spreadsheet file metadata.
/// A named formula, optionally scoped to one sheet by its zero-based index.
///
/// Despite the name, its formula may return a scalar or expression as well as a range.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NamedRange {
    /// Defined-name spelling used for scoped lookup.
    pub name: String,
    /// Normalized expression backing the defined name.
    pub formula: String,
    /// Local sheet index, or None for workbook scope.
    pub sheet_scope: Option<usize>,
}
