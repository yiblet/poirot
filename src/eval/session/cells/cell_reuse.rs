//! The reuse decision for one cell read.

use crate::model::CellValue;

/// Reuse either produces a value or requires execution, possibly with retained evidence.
pub(super) enum CellReuse {
    Value(CellValue),
    Execute { needs_validation: bool },
}
