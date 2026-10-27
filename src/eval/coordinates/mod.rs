//! Workbook cell coordinates: the `EvaluationTarget` cell identity and the run-local
//! hash tables and memo keyed by sheet index and address.
//!
//! Depends on: no other eval folder.

/// Hash map keyed by validated worksheet coordinates.
mod cell_map;
/// Coordinate-keyed memo of one run's evaluated cells.
mod cell_memo;
/// Hash set of validated worksheet coordinates.
mod cell_set;
/// A cell identified by a zero-based workbook sheet index and row/column address.
mod evaluation_target;

pub(in crate::eval) use cell_map::CellMap;
pub use cell_memo::CellMemo;
pub(in crate::eval) use cell_set::CellSet;
pub use evaluation_target::EvaluationTarget;
