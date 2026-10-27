/// A reference present in formula syntax, whether or not execution reads it.
///
/// Used to guard compiled plans, not to record runtime dependencies. A reference
/// in an unselected branch can appear here without being read during calculation.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PotentialReference {
    /// Syntactically present cell reference, possibly in an unselected branch.
    Cell(poirot_parsing::formula::CellReference),
    /// Ordered sheet endpoints and cell anchors retained until workbook resolution.
    SheetSpan(poirot_parsing::formula::SheetSpanReference),
    /// Syntactically present whole-column reference.
    Column {
        /// Zero-based column index.
        column: u32,
        /// Absolute translation flag.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
    /// Syntactically present whole-row reference.
    Row {
        /// Zero-based row index.
        row: u32,
        /// Absolute translation flag.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
}
