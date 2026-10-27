//! Bitwise result identity used by incremental consumers.
use crate::model::CellValue;

/// Result identity for incremental consumers. Spreadsheet comparisons intentionally
/// have different semantics: signed zero and NaN payloads remain distinct here.
pub fn same_result_identity(left: &CellValue, right: &CellValue) -> bool {
    match (left, right) {
        (CellValue::Number(left), CellValue::Number(right)) => left.to_bits() == right.to_bits(),
        _ => left == right,
    }
}
