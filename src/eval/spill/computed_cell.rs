use crate::eval::values::ArrayView;
use crate::model::CellValue;

/// A completed worksheet value and its optional unpublished rectangular footprint.
pub(in crate::eval) struct ComputedCell {
    pub(in crate::eval) value: CellValue,
    pub(in crate::eval) rows: Option<ArrayView>,
}

impl ComputedCell {
    /// Carry a scalar result without a generated footprint.
    pub(in crate::eval) fn scalar(value: CellValue) -> Self {
        Self { value, rows: None }
    }
}
