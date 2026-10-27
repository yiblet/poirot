//! Matrix operation failures.

use crate::scalar::CellError;

/// Matrix formula error, resource-budget refusal, or unsupported reference-source behavior.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MatrixError {
    /// Catchable spreadsheet failure from matrix validation or arithmetic.
    #[error("matrix formula evaluates to {0}")]
    Formula(CellError),
    /// Operational refusal before exceeding the matrix resource budget.
    #[error("matrix resource budget exceeded")]
    BudgetExceeded,
    /// A checked shape or work estimate cannot be represented.
    #[error("matrix shape or work estimate overflows")]
    SizeOverflow,
    /// A fallible storage reservation was refused by the allocator.
    #[error("matrix storage allocation failed")]
    AllocationFailure,
}
