//! Borrowed reference geometry decoded from bytecode or a bound tree.

/// Borrowed decoded reference geometry.
pub(in crate::eval) enum ReferenceRef<'a> {
    /// Single-cell coordinates with an optional sheet qualifier.
    Cell {
        /// Zero-based row.
        row: u32,
        /// Zero-based column.
        column: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Whole-column reference.
    Column {
        /// Zero-based column.
        column: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Whole-row reference.
    Row {
        /// Zero-based row.
        row: u32,
        /// Optional sheet spelling.
        sheet: Option<&'a str>,
    },
    /// Reference across ordered sheet endpoints.
    SheetSpan {
        /// First sheet spelling.
        first_sheet: &'a str,
        /// Last sheet spelling.
        last_sheet: &'a str,
        /// First cell endpoint.
        start: &'a poirot_parsing::formula::CellReference,
        /// Optional second cell endpoint.
        end: Option<&'a poirot_parsing::formula::CellReference>,
    },
}
