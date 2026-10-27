//! Decoded formula source placed on a sheet.
use crate::CellAddress;

/// Reconstructed source formula assigned to one sheet coordinate.
pub(super) struct SheetFormula {
    /// Sheet index in workbook order.
    pub(super) sheet: usize,
    /// Cell holding the formula.
    pub(super) address: CellAddress,
    /// Reconstructed formula source.
    pub(super) text: String,
}
