//! Matrix and array calculations for spreadsheet functions.
//!
//! Use [`Matrix`] with [`transpose`], [`multiply`], [`determinant`], or [`inverse`]. Other
//! operations include [`frequency`], [`sumproduct`], and [`trend`].
//!
//! The caller supplies arrays and a [`Budget`] for cell counts and estimated work.
//! Exceeding a budget is reported separately from spreadsheet errors.
mod trim;
pub use trim::{TrimBounds, TrimMode, TrimRectangle};
mod collection;
mod flatten;
pub use flatten::{Ignore, MemberKind, ScanOrder, flatten};
mod least_squares;
mod linear_algebra;
mod regression;
pub use least_squares::{Diagnostics, Fit, Intercept, Statistic, least_squares, regression_budget};
mod reshape;
mod selection;
use poirot_functions_core::CellError;
pub use reshape::{Axis, Transform};
pub use selection::{
    FieldRelationship, Group, Partition, SortOrder, filter, group_layout, hierarchy, partition,
    select, selection_work, sort, unique,
};

/// Matrix formula error, resource-budget refusal, or unsupported reference-source behavior.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// Catchable spreadsheet failure from matrix validation or arithmetic.
    Formula(CellError),
    /// Operational refusal before exceeding the matrix resource budget.
    BudgetExceeded,
    /// A checked shape or work estimate cannot be represented.
    SizeOverflow,
    /// A fallible storage reservation was refused by the allocator.
    AllocationFailure,
    /// The pinned POI implementation explicitly throws NotImplementedException.
    SourceUnsupported(&'static str),
}
/// Matrix result preserving formula errors, resource refusal, and unsupported source behavior.
pub type Result<T> = std::result::Result<T, Error>;
fn value<T>() -> Result<T> {
    Err(Error::Formula(CellError::Value))
}
fn num<T>() -> Result<T> {
    Err(Error::Formula(CellError::Num))
}
fn reference<T>() -> Result<T> {
    Err(Error::Formula(CellError::Ref))
}

/// Limits on retained elements and estimated work for a matrix operation.
///
/// An overflowing size or work estimate is rejected.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    /// Maximum logical elements admitted for one matrix operation.
    pub max_cells: usize,
    /// Maximum estimated kernel work admitted before execution.
    pub max_work: usize,
}
impl Budget {
    /// Reject overflowing estimates or requests above either resource limit.
    pub fn check(self, cells: Option<usize>, work: Option<usize>) -> Result<()> {
        match (cells, work) {
            (Some(c), Some(w)) if c <= self.max_cells && w <= self.max_work => Ok(()),
            (Some(_), Some(_)) => Err(Error::BudgetExceeded),
            _ => Err(Error::SizeOverflow),
        }
    }
}

/// A nonempty rectangular matrix stored in row-major order.
///
/// [`Matrix::new`] checks that the dimensions match the storage. Indices are
/// zero-based.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    rows: usize,
    cols: usize,
    values: Vec<T>,
}
impl<T> Matrix<T> {
    /// Create a matrix from row-major values.
    ///
    /// Returns a value error if either dimension is zero or `rows * cols` does not
    /// equal the number of values, including when that product overflows.
    pub fn new(rows: usize, cols: usize, values: Vec<T>) -> Result<Self> {
        if rows == 0 || cols == 0 || rows.checked_mul(cols) != Some(values.len()) {
            return value();
        }
        Ok(Self { rows, cols, values })
    }
    /// Return the number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }
    /// Return the number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }
    /// Borrow flattened row-major elements.
    pub fn values(&self) -> &[T] {
        &self.values
    }
    /// Consume the matrix and return its row-major storage.
    pub fn into_values(self) -> Vec<T> {
        self.values
    }
    /// Read an element, returning a formula error for invalid coordinates.
    pub fn get(&self, row: usize, col: usize) -> Result<&T> {
        if row >= self.rows || col >= self.cols {
            return value();
        }
        let index = row
            .checked_mul(self.cols)
            .and_then(|start| start.checked_add(col))
            .ok_or(Error::Formula(CellError::Value))?;
        self.values
            .get(index)
            .ok_or(Error::Formula(CellError::Value))
    }
}
fn finite(values: &[f64]) -> Result<()> {
    if values.iter().all(|x| x.is_finite()) {
        Ok(())
    } else {
        num()
    }
}
/// Transpose a matrix after checking output size and work limits.
pub fn transpose<T: Clone>(m: &Matrix<T>, budget: Budget) -> Result<Matrix<T>> {
    linear_algebra::transpose(m, budget)
}

/// Multiply matrices whose inner dimensions match.
///
/// Returns a value error if `a.cols() != b.rows()`.
pub fn multiply(a: &Matrix<f64>, b: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
    linear_algebra::multiply(a, b, budget)
}

/// Compute a square matrix determinant under the supplied budget.
pub fn determinant(m: &Matrix<f64>, budget: Budget) -> Result<f64> {
    linear_algebra::determinant(m, budget)
}

/// Compute a square matrix inverse, rejecting singular or invalid inputs.
pub fn inverse(m: &Matrix<f64>, budget: Budget) -> Result<Matrix<f64>> {
    linear_algebra::inverse(m, budget)
}

/// Count data values in the supplied bins.
///
/// POI uses Java binarySearch directly, including its behavior for duplicate or
/// unsorted bins. Do not sort or deduplicate bins in the adapter.
pub fn frequency(data: &[f64], bins: &[f64], budget: Budget) -> Result<Matrix<f64>> {
    linear_algebra::frequency(data, bins, budget)
}

/// Uncoerced matrix operand; adapters retain type information for collection rules.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// Numeric member available to numeric matrix collection.
    Number(f64),
    /// Boolean member retained for source-specific collection rules.
    Bool(bool),
    /// Blank member retained for source-specific collection rules.
    Blank,
    /// Text member retained until the selected collection rule handles it.
    Text(String),
    /// Spreadsheet error retained for source-specific error precedence.
    Error(CellError),
}
/// Distinguish direct scalars, cell references, and areas during matrix collection.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Direct scalar argument.
    Scalar(Cell),
    /// Single-sheet cell reference; preserves first-argument error precedence.
    Reference(Cell),
    /// Rectangular area whose members use matrix collection rules.
    Area(Matrix<Cell>),
}

pub use poirot_coercionv2::collections::MatrixCollection;

/// Admit a numeric matrix without changing coordinates under `SquareNumeric`. Other
/// policies preserve pinned POI compaction and padding of skipped members.
pub fn coercev2_matrix_collection(
    area: &Matrix<Cell>,
    policy: MatrixCollection,
    budget: Budget,
) -> Result<Matrix<f64>> {
    collection::coercev2_matrix_collection(area, policy, budget)
}

/// TREND rejects any non NumericValueEval (including source error values).
pub fn coercev2_trend_collection(area: &Matrix<Cell>, budget: Budget) -> Result<Matrix<f64>> {
    collection::coercev2_trend_collection(area, budget)
}

/// Multiply corresponding admitted elements and sum their products.
pub fn sumproduct(args: &[Input], budget: Budget) -> Result<f64> {
    collection::sumproduct(args, budget)
}

/// Observation requirement for regression prediction, independent of residual statistics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrendFitPolicy {
    /// Preserve the pinned POI minimum sizes and constant-predictor shortcut.
    PinnedPoi,
    /// Use the modern rank-aware fit, including determined and redundant designs.
    Determined,
}

/// Least squares with Householder QR (never the unstable normal equations).
/// Optional inputs represent missing arguments, not blank cells. The adapter
/// must require numeric TREND elements and a literal boolean fourth argument.
pub fn trend(
    y: &Matrix<f64>,
    known_x: Option<&Matrix<f64>>,
    new_x: Option<&Matrix<f64>>,
    constant: bool,
    budget: Budget,
) -> Result<Matrix<f64>> {
    regression::trend(
        y,
        known_x,
        new_x,
        constant,
        budget,
        TrendFitPolicy::PinnedPoi,
    )
}

/// Fit a trend under an explicit prediction sample requirement.
pub fn trend_with_policy(
    y: &Matrix<f64>,
    known_x: Option<&Matrix<f64>>,
    new_x: Option<&Matrix<f64>>,
    constant: bool,
    budget: Budget,
    policy: TrendFitPolicy,
) -> Result<Matrix<f64>> {
    regression::trend(y, known_x, new_x, constant, budget, policy)
}

impl<'a> From<&'a Cell> for poirot_coercionv2::compatibility::ScalarRef<'a> {
    fn from(value: &'a Cell) -> Self {
        match value {
            Cell::Number(n) => Self::Number(*n),
            Cell::Bool(b) => Self::Boolean(*b),
            Cell::Text(t) => Self::Text(poirot_coercionv2::TextRef::Utf8(t)),
            Cell::Blank => Self::Blank,
            Cell::Error(e) => Self::Error(*e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const B: Budget = Budget {
        max_cells: 10000,
        max_work: 100000,
    };
    fn m(r: usize, c: usize, v: &[f64]) -> Matrix<f64> {
        Matrix::new(r, c, v.to_vec()).unwrap()
    }
    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
    }
    #[test]
    fn matrix_access_rejects_invalid_and_overflowing_coordinates() {
        let matrix = Matrix::new(2, 2, vec![1., 2., 3., 4.]).unwrap();
        assert_eq!(matrix.get(1, 1), Ok(&4.));
        for (row, column) in [(2, 0), (0, 2), (usize::MAX, 0), (0, usize::MAX)] {
            assert_eq!(
                matrix.get(row, column),
                Err(Error::Formula(CellError::Value))
            );
        }
    }
    #[test]
    fn rectangular_product_and_transpose() {
        let a = m(2, 3, &[1., 2., 3., 4., 5., 6.]);
        let b = m(3, 2, &[7., 8., 9., 10., 11., 12.]);
        assert_eq!(
            multiply(&a, &b, B).unwrap(),
            m(2, 2, &[58., 64., 139., 154.])
        );
        assert_eq!(
            transpose(&a, B).unwrap(),
            m(3, 2, &[1., 4., 2., 5., 3., 6.])
        );
        assert_eq!(multiply(&a, &a, B), Err(Error::Formula(CellError::Value)));
    }
    #[test]
    fn pivoted_inverse_and_determinant() {
        let a = m(3, 3, &[0., 2., 1., 1., 1., 0., 2., 0., 1.]);
        close(determinant(&a, B).unwrap(), -4.);
        let identity = multiply(&a, &inverse(&a, B).unwrap(), B).unwrap();
        for i in 0..9 {
            close(identity.values[i], if i % 4 == 0 { 1. } else { 0. });
        }
        let singular = m(2, 2, &[1., 2., 2., 4.]);
        assert_eq!(determinant(&singular, B), Ok(0.));
        assert_eq!(inverse(&singular, B), Err(Error::Formula(CellError::Num)));
    }
    #[test]
    fn histogram_boundaries_and_empty() {
        assert_eq!(
            frequency(&[0., 1., 2., 3., 4.], &[1., 3.], B).unwrap(),
            m(3, 1, &[2., 2., 1.])
        );
        assert_eq!(
            frequency(&[], &[1., 3.], B).unwrap(),
            m(3, 1, &[0., 0., 0.])
        );
        assert_eq!(frequency(&[1., 2.], &[], B).unwrap(), m(1, 1, &[2.]));
    }
    #[test]
    fn budgets_are_operational_errors() {
        let b = Budget {
            max_cells: 1,
            max_work: 1,
        };
        assert_eq!(
            inverse(&m(2, 2, &[1., 0., 0., 1.]), b),
            Err(Error::BudgetExceeded)
        );
        assert_eq!(frequency(&[], &[1.], b), Err(Error::BudgetExceeded));
    }
    #[test]
    fn sumproduct_area_text_and_error_precedence() {
        let a =
            Input::Area(Matrix::new(1, 2, vec![Cell::Number(2.), Cell::Text("3".into())]).unwrap());
        let b = Input::Area(Matrix::new(1, 2, vec![Cell::Number(4.), Cell::Number(5.)]).unwrap());
        assert_eq!(sumproduct(&[a.clone(), b], B), Ok(8.));
        let error = Input::Area(Matrix::new(1, 1, vec![Cell::Error(CellError::NA)]).unwrap());
        assert_eq!(
            sumproduct(&[a, error], B),
            Err(Error::Formula(CellError::NA))
        );
        assert_eq!(
            sumproduct(
                &[
                    Input::Scalar(Cell::Number(2.)),
                    Input::Scalar(Cell::Text("3".into()))
                ],
                B
            ),
            Err(Error::Formula(CellError::Value))
        );
    }
    #[test]
    fn pinned_area_collectors_preserve_compaction_and_error_rules() {
        let a = Matrix::new(
            2,
            2,
            vec![
                Cell::Number(2.),
                Cell::Text("8".into()),
                Cell::Number(3.),
                Cell::Blank,
            ],
        )
        .unwrap();
        assert_eq!(
            coercev2_matrix_collection(&a, MatrixCollection::Multiply, B).unwrap(),
            m(2, 2, &[2., 3., 0., 0.])
        );
        assert_eq!(
            coercev2_matrix_collection(&a, MatrixCollection::Determinant, B),
            Err(Error::Formula(CellError::Value))
        );
        assert_eq!(
            coercev2_matrix_collection(&a, MatrixCollection::Transpose, B).unwrap(),
            m(2, 2, &[2., 3., 0., 0.])
        );
        assert_eq!(
            coercev2_trend_collection(&a, B),
            Err(Error::Formula(CellError::Value))
        );
        let one = Matrix::new(1, 1, vec![Cell::Number(2.)]).unwrap();
        assert_eq!(
            coercev2_matrix_collection(&one, MatrixCollection::Inverse, B),
            Err(Error::Formula(CellError::Value))
        );
        let error = Matrix::new(1, 1, vec![Cell::Error(CellError::NA)]).unwrap();
        assert_eq!(
            coercev2_trend_collection(&error, B),
            Err(Error::Formula(CellError::Value))
        );
    }
    #[test]
    fn numeric_matrix_admission_preserves_coordinates_and_rejects_nonnumbers() {
        let valid = Matrix::new(
            2,
            2,
            vec![
                Cell::Number(0.),
                Cell::Number(2.),
                Cell::Number(3.),
                Cell::Number(4.),
            ],
        )
        .unwrap();
        let admitted =
            coercev2_matrix_collection(&valid, MatrixCollection::SquareNumeric, B).unwrap();
        assert_eq!(admitted, m(2, 2, &[0., 2., 3., 4.]));
        assert_eq!(determinant(&admitted, B), Ok(-6.));
        for invalid in [
            Cell::Blank,
            Cell::Text("2".into()),
            Cell::Text("x".into()),
            Cell::Bool(true),
            Cell::Bool(false),
        ] {
            for index in 0..4 {
                let mut cells = valid.values.clone();
                cells[index] = invalid.clone();
                let input = Matrix::new(2, 2, cells).unwrap();
                assert_eq!(
                    coercev2_matrix_collection(&input, MatrixCollection::SquareNumeric, B),
                    Err(Error::Formula(CellError::Value))
                );
            }
        }
        let singleton = Matrix::new(1, 1, vec![Cell::Number(2.)]).unwrap();
        let admitted =
            coercev2_matrix_collection(&singleton, MatrixCollection::SquareNumeric, B).unwrap();
        assert_eq!(determinant(&admitted, B), Ok(2.));
        assert_eq!(inverse(&admitted, B), Ok(m(1, 1, &[0.5])));
        for policy in [MatrixCollection::Determinant, MatrixCollection::Inverse] {
            assert_eq!(
                coercev2_matrix_collection(&singleton, policy, B),
                Err(Error::Formula(CellError::Value))
            );
        }
        for (rows, cols, values) in [
            (0, 2, vec![]),
            (2, 2, vec![1., 2., 3.]),
            (usize::MAX, 2, vec![]),
        ] {
            assert_eq!(
                Matrix::new(rows, cols, values),
                Err(Error::Formula(CellError::Value))
            );
        }
        let nonsquare = m(1, 2, &[1., 2.]);
        assert_eq!(
            determinant(&nonsquare, B),
            Err(Error::Formula(CellError::Value))
        );
        assert_eq!(
            inverse(&nonsquare, B),
            Err(Error::Formula(CellError::Value))
        );
        assert_eq!(
            coercev2_matrix_collection(
                &valid,
                MatrixCollection::SquareNumeric,
                Budget {
                    max_cells: 3,
                    max_work: 100
                }
            ),
            Err(Error::BudgetExceeded)
        );
        assert_eq!(
            coercev2_matrix_collection(
                &valid,
                MatrixCollection::SquareNumeric,
                Budget {
                    max_cells: 4,
                    max_work: 3
                }
            ),
            Err(Error::BudgetExceeded)
        );
    }
    #[test]
    fn numeric_matrix_admission_rejects_invalid_members_before_first_embedded_error() {
        for cells in [
            vec![
                Cell::Number(1.),
                Cell::Text("x".into()),
                Cell::Number(3.),
                Cell::Error(CellError::NA),
            ],
            vec![
                Cell::Number(1.),
                Cell::Error(CellError::NA),
                Cell::Number(3.),
                Cell::Blank,
            ],
        ] {
            let input = Matrix::new(2, 2, cells).unwrap();
            assert_eq!(
                coercev2_matrix_collection(&input, MatrixCollection::SquareNumeric, B),
                Err(Error::Formula(CellError::Value))
            );
        }
        for (first, second) in [
            (CellError::Div0, CellError::NA),
            (CellError::NA, CellError::Div0),
        ] {
            let input = Matrix::new(
                2,
                2,
                vec![
                    Cell::Number(1.),
                    Cell::Error(first),
                    Cell::Number(3.),
                    Cell::Error(second),
                ],
            )
            .unwrap();
            assert_eq!(
                coercev2_matrix_collection(&input, MatrixCollection::SquareNumeric, B),
                Err(Error::Formula(first))
            );
        }
    }
    #[test]
    fn trend_regression_orientation_and_origin() {
        let y = m(1, 3, &[3., 5., 7.]);
        assert_eq!(trend(&y, None, None, true, B).unwrap().rows(), 1);
        let result = trend(&y, None, Some(&m(1, 2, &[4., 5.])), true, B).unwrap();
        close(result.values[0], 9.);
        close(result.values[1], 11.);
        let result = trend(
            &m(3, 1, &[2., 4., 6.]),
            None,
            Some(&m(1, 2, &[4., 5.])),
            false,
            B,
        )
        .unwrap();
        close(result.values[0], 8.);
        close(result.values[1], 10.);
    }
    #[test]
    fn trend_multiple_predictors() {
        // Independent full-rank design, y = 1 + 2*x1 + 3*x2.
        let x = m(4, 2, &[0., 0., 1., 0., 0., 1., 1., 1.]);
        let y = m(4, 1, &[1., 3., 4., 6.]);
        let result = trend(&y, Some(&x), Some(&m(1, 2, &[2., 3.])), true, B).unwrap();
        close(result.values[0], 14.);
    }
}

/// Generate row-major sequential numbers with admitted dimensions and resource limits.
/// Defaults and numeric coercion belong to the caller. Microsoft documents the
/// shape, ordering and optional arguments at
/// <https://support.microsoft.com/en-us/excel/functions/sequence-function>.
/// This implementation and its mathematical tests were independently written.
pub fn sequence(
    rows: usize,
    columns: usize,
    start: f64,
    step: f64,
    budget: Budget,
) -> Result<Matrix<f64>> {
    if rows == 0 || columns == 0 {
        return value();
    }
    if !start.is_finite() || !step.is_finite() {
        return num();
    }
    let count = rows.checked_mul(columns).ok_or(Error::SizeOverflow)?;
    budget.check(Some(count), Some(count))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    for index in 0..count {
        let number = start + step * index as f64;
        if !number.is_finite() {
            return num();
        }
        // SAFETY: The reservation covers count pushes and this loop performs exactly count.
        values.push(number);
    }
    Matrix::new(rows, columns, values)
}

#[cfg(test)]
mod sequence_tests {
    use super::*;
    #[test]
    fn row_major_generation_matches_independent_linear_coordinates() {
        let budget = Budget {
            max_cells: 100,
            max_work: 100,
        };
        for rows in 1..=7 {
            for columns in 1..=5 {
                let matrix = sequence(rows, columns, 5., -2., budget).unwrap();
                assert_eq!((matrix.rows(), matrix.cols()), (rows, columns));
                for row in 0..rows {
                    for column in 0..columns {
                        assert_eq!(
                            matrix.values()[row * columns + column],
                            5. - 2. * (row * columns + column) as f64
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn sequence_rejects_invalid_counts_work_limits_and_nonfinite_numbers() {
        let budget = Budget {
            max_cells: 4,
            max_work: 4,
        };
        assert_eq!(
            sequence(0, 1, 1., 1., budget),
            Err(Error::Formula(CellError::Value))
        );
        assert_eq!(
            sequence(usize::MAX, 2, 1., 1., budget),
            Err(Error::SizeOverflow)
        );
        assert_eq!(sequence(3, 2, 1., 1., budget), Err(Error::BudgetExceeded));
        assert_eq!(
            sequence(
                2,
                2,
                1.,
                1.,
                Budget {
                    max_cells: 4,
                    max_work: 3
                }
            ),
            Err(Error::BudgetExceeded)
        );
        assert_eq!(
            sequence(1, 1, f64::INFINITY, 1., budget),
            Err(Error::Formula(CellError::Num))
        );
        assert_eq!(
            sequence(2, 1, 1E308, 1E308, budget),
            Err(Error::Formula(CellError::Num))
        );
    }
}

/// Generate a numeric identity matrix after admitting its square geometry.
pub fn unit(dimension: usize, budget: Budget) -> Result<Matrix<f64>> {
    if dimension == 0 {
        return value();
    }
    let count = dimension
        .checked_mul(dimension)
        .ok_or(Error::SizeOverflow)?;
    budget.check(Some(count), Some(count))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    for row in 0..dimension {
        for column in 0..dimension {
            // SAFETY: count = dimension squared was checked and reserved above;
            // these two loops push exactly count entries.
            values.push(f64::from(row == column));
        }
    }
    Matrix::new(dimension, dimension, values)
}
