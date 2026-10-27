//! Sparse scenario inputs and the effective workbook reads that merge them with a
//! baseline source.
//!
//! Depends on: the workbook model only.

/// One authored mutation of a cell.
mod cell_edit;
/// Merged column traversal of baseline and override cursors.
mod column_traversal;
/// Merged iteration over effective stored cells.
mod effective_stored_cells;
/// Read view combining a baseline workbook with scenario inputs.
mod effective_workbook;
/// One sheet's sparse exclusive edits.
mod input_cells;
/// Column-ordered cursor over authoritative edits.
mod input_column;
/// Merged base and overlay source iteration.
mod overlay_cells;
/// Bounded merged iteration for sparse range consumers.
mod overlay_range;
/// Sparse scenario assignments bound to one workbook source.
mod run_inputs;
/// A merge cursor's borrowed source before tombstone filtering.
mod stored_source;
#[cfg(test)]
mod tests;

pub(in crate::eval) use cell_edit::CellEdit;
pub use effective_stored_cells::EffectiveStoredCells;
pub use effective_workbook::EffectiveWorkbook;
pub use run_inputs::RunInputs;
