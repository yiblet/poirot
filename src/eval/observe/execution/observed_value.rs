use crate::model::CellValue;

/// Borrowed result evidence: scalar data, omitted syntax, or array/reference shape.
/// Shape evidence does not expose every array element or reference operand.
#[derive(Clone, Copy, Debug)]
pub enum ObservedValue<'a> {
    /// Borrowed scalar evidence, including spreadsheet errors.
    Scalar(&'a CellValue),
    /// Omitted-argument evidence.
    Missing,
    /// Shape summary without copying array elements.
    Array {
        /// Number of materialized rows.
        rows: usize,
        /// Total number of materialized elements.
        values: usize,
    },
    /// Reference shape without reading its cells.
    Reference {
        /// Number of reference areas.
        areas: usize,
        /// Whether this is a true multi-sheet reference.
        sheet_span: bool,
    },
}
