//! Independently written array geometry; Microsoft function contracts are linked in tests.
use crate::matrix::{Budget, Matrix, MatrixError, Result};
use crate::scalar::CellError;

/// The coordinate direction selected or concatenated by an array operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Change rows while retaining columns.
    Rows,
    /// Change columns while retaining rows.
    Columns,
}
/// A checked array transformation, independent of element types and workbook state.
#[derive(Clone, Debug)]
pub enum Transform {
    /// Append matrices, padding the shorter perpendicular extent.
    Stack(Axis),
    /// Wrap a vector into rows or columns with a maximum extent.
    Wrap {
        /// Whether to fill each row or each column before starting the next.
        axis: Axis,
        /// Maximum elements in each filled row or column; zero returns NUM.
        count: usize,
    },
    /// Retain signed counts from either end; omitted axes remain unchanged.
    Take {
        /// Rows retained from the start (positive) or end (negative); `None` keeps all.
        rows: Option<i64>,
        /// Columns retained from the start (positive) or end (negative); `None` keeps all.
        columns: Option<i64>,
    },
    /// Remove signed counts from either end; omitted axes remain unchanged.
    Drop {
        /// Rows removed from the start (positive) or end (negative); `None` removes none.
        rows: Option<i64>,
        /// Columns removed from the start (positive) or end (negative); `None` removes none.
        columns: Option<i64>,
    },
    /// Select signed one-based coordinates, preserving order and repetitions.
    Choose {
        /// Whether each index selects a row or a column.
        axis: Axis,
        /// One-based positions from the start (positive) or end (negative).
        /// Empty lists, zero and out-of-bounds positions return VALUE.
        indices: Vec<i64>,
    },
    /// Retain the original rectangle and pad to the supplied dimensions.
    Expand {
        /// Output row count, at least the input row count.
        rows: usize,
        /// Output column count, at least the input column count.
        columns: usize,
    },
}
fn formula<T>(error: CellError) -> Result<T> {
    Err(MatrixError::Formula(error))
}
fn window(length: usize, count: Option<i64>, drop: bool) -> Result<(usize, usize)> {
    let Some(count) = count else {
        return Ok((0, length));
    };
    if count == 0 {
        return formula(CellError::Calc);
    }
    let amount = usize::try_from(count.unsigned_abs())
        .unwrap_or(usize::MAX)
        .min(length);
    let size = if drop { length - amount } else { amount };
    if size == 0 {
        return formula(CellError::Calc);
    }
    let start = if (count < 0) != drop {
        length - size
    } else {
        0
    };
    Ok((start, size))
}
fn selected(length: usize, index: i64) -> Result<usize> {
    let amount = usize::try_from(index.unsigned_abs())
        .map_err(|_| MatrixError::Formula(CellError::Value))?;
    if amount == 0 || amount > length {
        return formula(CellError::Value);
    }
    Ok(if index < 0 {
        length - amount
    } else {
        amount - 1
    })
}
impl Transform {
    /// Validate the operands and return the output dimensions before allocation.
    pub fn shape<T>(&self, inputs: &[Matrix<T>]) -> Result<(usize, usize)> {
        let first = inputs
            .first()
            .ok_or(MatrixError::Formula(CellError::Value))?;
        if let Self::Stack(axis) = self {
            let mut rows = 0usize;
            let mut columns = 0usize;
            for input in inputs {
                match axis {
                    Axis::Rows => {
                        rows = rows
                            .checked_add(input.rows())
                            .ok_or(MatrixError::SizeOverflow)?;
                        columns = columns.max(input.cols());
                    }
                    Axis::Columns => {
                        columns = columns
                            .checked_add(input.cols())
                            .ok_or(MatrixError::SizeOverflow)?;
                        rows = rows.max(input.rows());
                    }
                }
            }
            return Ok((rows, columns));
        }
        if inputs.len() != 1 {
            return formula(CellError::Value);
        }
        match self {
            Self::Take { rows, columns } | Self::Drop { rows, columns } => {
                let drop = matches!(self, Self::Drop { .. });
                Ok((
                    window(first.rows(), *rows, drop)?.1,
                    window(first.cols(), *columns, drop)?.1,
                ))
            }
            Self::Choose { axis, indices } => {
                if indices.is_empty() {
                    return formula(CellError::Value);
                }
                let length = match axis {
                    Axis::Rows => first.rows(),
                    Axis::Columns => first.cols(),
                };
                for index in indices {
                    selected(length, *index)?;
                }
                Ok(match axis {
                    Axis::Rows => (indices.len(), first.cols()),
                    Axis::Columns => (first.rows(), indices.len()),
                })
            }
            Self::Wrap { axis, count } => {
                if first.rows() != 1 && first.cols() != 1 {
                    return formula(CellError::Value);
                }
                if *count == 0 {
                    return formula(CellError::Num);
                }
                let extent = (*count).min(first.values().len());
                let groups = first
                    .values()
                    .len()
                    .checked_div(extent)
                    .and_then(|whole| {
                        whole.checked_add(usize::from(
                            first.values().len().checked_rem(extent)? != 0,
                        ))
                    })
                    .ok_or(MatrixError::Formula(CellError::Value))?;
                Ok(match axis {
                    Axis::Rows => (groups, extent),
                    Axis::Columns => (extent, groups),
                })
            }
            Self::Expand { rows, columns } => {
                if *rows < first.rows() || *columns < first.cols() {
                    return formula(CellError::Value);
                }
                Ok((*rows, *columns))
            }
            Self::Stack(_) => formula(CellError::Value),
        }
    }
    /// Apply a transformation after checking output storage and work limits.
    pub fn apply<T: Clone>(
        &self,
        inputs: &[Matrix<T>],
        padding: &T,
        budget: Budget,
    ) -> Result<Matrix<T>> {
        let (rows, columns) = self.shape(inputs)?;
        let count = rows.checked_mul(columns).ok_or(MatrixError::SizeOverflow)?;
        budget.check(Some(count), Some(count))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| MatrixError::AllocationFailure)?;
        if let Self::Stack(axis) = self {
            match axis {
                Axis::Columns => {
                    for row in 0..rows {
                        for input in inputs {
                            for column in 0..input.cols() {
                                values.push(input.get(row, column).unwrap_or(padding).clone());
                            }
                        }
                    }
                }
                Axis::Rows => {
                    for input in inputs {
                        for row in 0..input.rows() {
                            for column in 0..columns {
                                values.push(input.get(row, column).unwrap_or(padding).clone());
                            }
                        }
                    }
                }
            }
            return Matrix::new(rows, columns, values);
        }
        let first = inputs
            .first()
            .ok_or(MatrixError::Formula(CellError::Value))?;
        for row in 0..rows {
            for column in 0..columns {
                let member = match self {
                    Self::Stack(_) => return formula(CellError::Value),
                    Self::Take {
                        rows: r,
                        columns: c,
                    }
                    | Self::Drop {
                        rows: r,
                        columns: c,
                    } => {
                        let drop = matches!(self, Self::Drop { .. });
                        Some(first.get(
                            window(first.rows(), *r, drop)?.0 + row,
                            window(first.cols(), *c, drop)?.0 + column,
                        )?)
                    }
                    Self::Choose { axis, indices } => {
                        let offset = match axis {
                            Axis::Rows => row,
                            Axis::Columns => column,
                        };
                        let index = *indices
                            .get(offset)
                            .ok_or(MatrixError::Formula(CellError::Value))?;
                        Some(match axis {
                            Axis::Rows => first.get(selected(first.rows(), index)?, column)?,
                            Axis::Columns => first.get(row, selected(first.cols(), index)?)?,
                        })
                    }
                    Self::Wrap { axis, .. } => {
                        let index = match axis {
                            Axis::Rows => {
                                row.checked_mul(columns).and_then(|n| n.checked_add(column))
                            }
                            Axis::Columns => {
                                column.checked_mul(rows).and_then(|n| n.checked_add(row))
                            }
                        }
                        .ok_or(MatrixError::SizeOverflow)?;
                        first.values().get(index)
                    }
                    Self::Expand { .. } => first.get(row, column).ok(),
                };
                values.push(member.unwrap_or(padding).clone());
            }
        }
        Matrix::new(rows, columns, values)
    }
}
