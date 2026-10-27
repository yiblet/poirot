use crate::model::{CellAddress, CellValue};

/// Materialized array values with the coordinate origin needed for scalar selection.
/// An absent anchor preserves the fact that a computed array has no established
/// worksheet position; consumers must not invent one for implicit intersection.
#[derive(Clone)]
pub(in crate::eval) struct ArrayView {
    pub(in crate::eval) values: Vec<Vec<CellValue>>,
    /// None means that the producing adapter has not established source coordinates.
    pub(in crate::eval) anchor: Option<CellAddress>,
}

impl ArrayView {
    pub(in crate::eval) fn unknown(values: Vec<Vec<CellValue>>) -> Self {
        Self {
            values,
            anchor: None,
        }
    }

    pub(in crate::eval) fn anchored(
        values: Vec<Vec<CellValue>>,
        anchor: Option<CellAddress>,
    ) -> Self {
        Self { values, anchor }
    }
}

impl std::ops::Deref for ArrayView {
    /// Row-major backing values borrowed by array-aware consumers.
    type Target = Vec<Vec<CellValue>>;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl IntoIterator for ArrayView {
    /// One owned row of array values.
    type Item = Vec<CellValue>;

    /// Consuming iterator over owned rows.
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}
