//! Original coordinate and filtering tests from Microsoft TOCOL/TOROW/WRAP contracts.
//! https://support.microsoft.com/en-us/excel/functions/tocol-function
//! https://support.microsoft.com/en-us/excel/functions/wraprows-function
use poirot_functions_core::CellError;
use poirot_functions_matrix::{
    Axis, Budget, Error, Ignore, Matrix, MemberKind, ScanOrder, Transform, flatten,
};
const B: Budget = Budget {
    max_cells: 100,
    max_work: 100,
};
#[test]
fn flatten_filters_original_values_in_each_scan_order() {
    let m = Matrix::new(2, 3, vec![0, 2, -1, 3, 4, 5]).unwrap();
    let classify = |n: &i32| match n {
        0 => MemberKind::Blank,
        -1 => MemberKind::Error,
        _ => MemberKind::Value,
    };
    for (ignore, expected) in [
        (Ignore::None, vec![0, 2, -1, 3, 4, 5]),
        (Ignore::Blanks, vec![2, -1, 3, 4, 5]),
        (Ignore::Errors, vec![0, 2, 3, 4, 5]),
        (Ignore::BlanksAndErrors, vec![2, 3, 4, 5]),
    ] {
        let out = flatten(&m, Axis::Columns, ScanOrder::Rows, ignore, classify, B).unwrap();
        assert_eq!(out.rows(), expected.len());
        assert_eq!(out.cols(), 1);
        assert_eq!(out.values(), expected);
    }
    let out = flatten(
        &m,
        Axis::Rows,
        ScanOrder::Columns,
        Ignore::BlanksAndErrors,
        classify,
        B,
    )
    .unwrap();
    assert_eq!(out.values(), [3, 2, 4, 5]);
    assert_eq!((out.rows(), out.cols()), (1, 4));
    let empty = Matrix::new(1, 1, vec![0]).unwrap();
    assert_eq!(
        flatten(
            &empty,
            Axis::Rows,
            ScanOrder::Rows,
            Ignore::Blanks,
            classify,
            B
        ),
        Err(Error::Formula(CellError::Calc))
    );
    assert_eq!(
        flatten(
            &m,
            Axis::Rows,
            ScanOrder::Rows,
            Ignore::None,
            classify,
            Budget { max_cells: 5, ..B }
        ),
        Err(Error::BudgetExceeded)
    );
    assert_eq!(
        flatten(
            &m,
            Axis::Rows,
            ScanOrder::Rows,
            Ignore::None,
            classify,
            Budget { max_work: 11, ..B }
        ),
        Err(Error::BudgetExceeded)
    );
}
#[test]
fn wraps_vectors_with_bounded_geometry_and_padding() {
    let input = Matrix::new(5, 1, vec![2, 3, 5, 7, 11]).unwrap();
    for (axis, shape, expected) in [
        (Axis::Rows, (3, 2), vec![2, 3, 5, 7, 11, -1]),
        (Axis::Columns, (2, 3), vec![2, 5, 11, 3, 7, -1]),
    ] {
        let out = Transform::Wrap { axis, count: 2 }
            .apply(std::slice::from_ref(&input), &-1, B)
            .unwrap();
        assert_eq!((out.rows(), out.cols()), shape);
        assert_eq!(out.values(), expected);
    }
    assert_eq!(
        Transform::Wrap {
            axis: Axis::Rows,
            count: usize::MAX
        }
        .shape(std::slice::from_ref(&input)),
        Ok((1, 5))
    );
    assert_eq!(
        Transform::Wrap {
            axis: Axis::Rows,
            count: 0
        }
        .shape(std::slice::from_ref(&input)),
        Err(Error::Formula(CellError::Num))
    );
    let rectangle = Matrix::new(2, 2, vec![1, 2, 3, 4]).unwrap();
    assert_eq!(
        Transform::Wrap {
            axis: Axis::Rows,
            count: 2
        }
        .shape(&[rectangle]),
        Err(Error::Formula(CellError::Value))
    );
    assert_eq!(
        Transform::Wrap {
            axis: Axis::Rows,
            count: 2
        }
        .apply(&[input], &0, Budget { max_cells: 5, ..B }),
        Err(Error::BudgetExceeded)
    );
}
