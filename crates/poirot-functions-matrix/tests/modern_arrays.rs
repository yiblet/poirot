//! Independently authored tests of Microsoft array contracts, retrieved 2026-10-21.
//! See https://support.microsoft.com/en-us/excel/functions/take-function and the
//! corresponding drop, hstack, vstack, choosecols, chooserows and expand pages.
use poirot_functions_core::CellError;
use poirot_functions_matrix::{Axis, Budget, Error, Matrix, Transform};
const B: Budget = Budget {
    max_cells: 100,
    max_work: 100,
};
fn input() -> Matrix<i32> {
    Matrix::new(2, 3, vec![2, 4, 6, 8, 10, 12]).unwrap()
}
#[test]
fn signed_selection_and_omission_preserve_coordinates() {
    for (t, rows, cols, expected) in [
        (
            Transform::Take {
                rows: Some(-1),
                columns: None,
            },
            1,
            3,
            vec![8, 10, 12],
        ),
        (
            Transform::Take {
                rows: None,
                columns: Some(-2),
            },
            2,
            2,
            vec![4, 6, 10, 12],
        ),
        (
            Transform::Drop {
                rows: Some(-1),
                columns: None,
            },
            1,
            3,
            vec![2, 4, 6],
        ),
        (
            Transform::Drop {
                rows: None,
                columns: Some(2),
            },
            2,
            1,
            vec![6, 12],
        ),
        (
            Transform::Take {
                rows: Some(99),
                columns: None,
            },
            2,
            3,
            vec![2, 4, 6, 8, 10, 12],
        ),
        (
            Transform::Choose {
                axis: Axis::Columns,
                indices: vec![-1, 1, -1],
            },
            2,
            3,
            vec![6, 2, 6, 12, 8, 12],
        ),
        (
            Transform::Choose {
                axis: Axis::Rows,
                indices: vec![-1, 1],
            },
            2,
            3,
            vec![8, 10, 12, 2, 4, 6],
        ),
    ] {
        assert_eq!(
            t.apply(&[input()], &-1, B).unwrap(),
            Matrix::new(rows, cols, expected).unwrap()
        );
    }
}
#[test]
fn stack_and_expand_pad_only_absent_coordinates() {
    let narrow = Matrix::new(1, 1, vec![9]).unwrap();
    assert_eq!(
        Transform::Stack(Axis::Columns)
            .apply(&[input(), narrow.clone()], &-1, B)
            .unwrap()
            .values(),
        &[2, 4, 6, 9, 8, 10, 12, -1]
    );
    assert_eq!(
        Transform::Stack(Axis::Rows)
            .apply(&[narrow, input()], &-1, B)
            .unwrap()
            .values(),
        &[9, -1, -1, 2, 4, 6, 8, 10, 12]
    );
    assert_eq!(
        Transform::Expand {
            rows: 3,
            columns: 4
        }
        .apply(&[input()], &-1, B)
        .unwrap()
        .values(),
        &[2, 4, 6, -1, 8, 10, 12, -1, -1, -1, -1, -1]
    );
}
#[test]
fn invalid_empty_shapes_and_budgets_are_distinct() {
    for t in [
        Transform::Take {
            rows: Some(0),
            columns: None,
        },
        Transform::Drop {
            rows: Some(2),
            columns: None,
        },
    ] {
        assert_eq!(t.shape(&[input()]), Err(Error::Formula(CellError::Calc)));
    }
    for t in [
        Transform::Choose {
            axis: Axis::Rows,
            indices: vec![0],
        },
        Transform::Choose {
            axis: Axis::Columns,
            indices: vec![4],
        },
        Transform::Expand {
            rows: 1,
            columns: 3,
        },
    ] {
        assert_eq!(t.shape(&[input()]), Err(Error::Formula(CellError::Value)));
    }
    assert_eq!(
        Transform::Expand {
            rows: usize::MAX,
            columns: 3
        }
        .apply(&[input()], &0, B),
        Err(Error::SizeOverflow)
    );
    assert_eq!(
        Transform::Stack(Axis::Rows).apply(
            &[input()],
            &0,
            Budget {
                max_cells: 5,
                max_work: 100
            }
        ),
        Err(Error::BudgetExceeded)
    );
}
