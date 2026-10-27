use crate::model::CellValue;

/// Owned scalar evidence or an array/reference shape summary copied from an observation.
#[derive(Clone, Debug)]
pub enum RetainedValue {
    /// Owned scalar evidence.
    Scalar(CellValue),
    /// Omitted-argument evidence.
    Missing,
    /// Array shape without retained elements.
    Array {
        /// Number of rows.
        rows: usize,
        /// Total element count.
        values: usize,
    },
    /// Reference shape without retained cell contents.
    Reference {
        /// Number of reference areas.
        areas: usize,
        /// Whether the reference spans sheets.
        sheet_span: bool,
    },
}
