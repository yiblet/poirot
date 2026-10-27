//! Independently authored coordinate and ordering tests; no upstream fixture material.
use poirot_functions::matrix::{
    Axis, Budget, Matrix, MatrixError, SortOrder, filter, select, sort, unique,
};
use poirot_functions::scalar::CellError;
const B: Budget = Budget {
    max_cells: 10000,
    max_work: 100000,
};
fn m(rows: usize, cols: usize, values: &[i32]) -> Matrix<i32> {
    Matrix::new(rows, cols, values.to_vec()).unwrap()
}
#[test]
fn selection_and_masks_preserve_coordinates_and_validate_geometry() {
    let input = m(3, 2, &[7, 8, 2, 3, 5, 6]);
    assert_eq!(
        filter(&input, Axis::Rows, &[true, false, true], B).unwrap(),
        m(2, 2, &[7, 8, 5, 6])
    );
    assert_eq!(
        filter(&input, Axis::Columns, &[false, true], B).unwrap(),
        m(3, 1, &[8, 3, 6])
    );
    assert_eq!(
        select(&input, Axis::Rows, &[2, 0, 2], B).unwrap(),
        m(3, 2, &[5, 6, 7, 8, 5, 6])
    );
    assert_eq!(
        filter(&input, Axis::Rows, &[false; 3], B),
        Err(MatrixError::Formula(CellError::Calc))
    );
    assert_eq!(
        filter(&input, Axis::Rows, &[true], B),
        Err(MatrixError::Formula(CellError::Value))
    );
    assert_eq!(
        select(&input, Axis::Columns, &[2], B),
        Err(MatrixError::Formula(CellError::Value))
    );
    assert_eq!(
        filter(&input, Axis::Rows, &[true; 3], Budget { max_cells: 5, ..B }),
        Err(MatrixError::BudgetExceeded)
    );
}
#[test]
fn lexicographic_sort_and_unique_retain_first_occurrence_order() {
    let input = m(5, 2, &[3, 8, 1, 9, 3, 8, 1, 4, 2, 7]);
    assert_eq!(
        sort(
            &input,
            Axis::Rows,
            &input,
            &[SortOrder::Ascending, SortOrder::Descending],
            i32::cmp,
            B
        )
        .unwrap(),
        m(5, 2, &[1, 9, 1, 4, 2, 7, 3, 8, 3, 8])
    );
    assert_eq!(
        unique(&input, Axis::Rows, false, i32::cmp, B).unwrap(),
        m(4, 2, &[3, 8, 1, 9, 1, 4, 2, 7])
    );
    assert_eq!(
        unique(&input, Axis::Rows, true, i32::cmp, B).unwrap(),
        m(3, 2, &[1, 9, 1, 4, 2, 7])
    );
    let columns = m(2, 4, &[3, 1, 3, 2, 8, 9, 8, 7]);
    assert_eq!(
        unique(&columns, Axis::Columns, false, i32::cmp, B).unwrap(),
        m(2, 3, &[3, 1, 2, 8, 9, 7])
    );
    assert_eq!(
        unique(&columns, Axis::Columns, true, i32::cmp, B).unwrap(),
        m(2, 2, &[1, 2, 9, 7])
    );
    assert_eq!(
        unique(&m(3, 1, &[4, 4, 4]), Axis::Rows, true, i32::cmp, B),
        Err(MatrixError::Formula(CellError::Calc))
    );
    assert_eq!(
        sort(
            &input,
            Axis::Rows,
            &m(1, 1, &[0]),
            &[SortOrder::Ascending],
            i32::cmp,
            B
        ),
        Err(MatrixError::Formula(CellError::Value))
    );
    assert_eq!(
        unique(
            &input,
            Axis::Rows,
            false,
            i32::cmp,
            Budget { max_work: 1, ..B }
        ),
        Err(MatrixError::BudgetExceeded)
    );
}
#[test]
fn unique_comparison_count_scales_below_quadratic() {
    let n = 2048;
    let input = Matrix::new(n, 1, (0..n).map(|i| (i % 31) as i32).collect()).unwrap();
    let comparisons = std::cell::Cell::new(0usize);
    let result = unique(
        &input,
        Axis::Rows,
        false,
        |a, b| {
            comparisons.set(comparisons.get() + 1);
            a.cmp(b)
        },
        Budget {
            max_cells: n,
            max_work: 100000,
        },
    )
    .unwrap();
    assert_eq!(result.rows(), 31);
    assert!(
        comparisons.get() < n * 64,
        "{} comparisons",
        comparisons.get()
    );
}
#[test]
fn inconsistent_comparators_remain_bounded_without_intrinsic_sort_panics() {
    use std::cmp::Ordering;
    let input = m(8, 1, &[1, 2, 3, 4, 5, 6, 7, 8]);
    let unstable = std::cell::Cell::new(false);
    let compare = |_: &i32, _: &i32| {
        unstable.set(!unstable.get());
        if unstable.get() {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    };
    let sorted = sort(
        &input,
        Axis::Rows,
        &input,
        &[SortOrder::Ascending],
        compare,
        B,
    )
    .unwrap();
    let mut result = sorted.into_values();
    result.sort();
    assert_eq!(result, input.values());
    let unstable = std::cell::Cell::new(false);
    let result = unique(
        &input,
        Axis::Rows,
        false,
        |_, _| {
            unstable.set(!unstable.get());
            if unstable.get() {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        },
        B,
    )
    .unwrap();
    assert_eq!(result.rows(), input.rows());
}
