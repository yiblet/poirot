//! Identity-matrix definition and bounded allocation contract.
use poirot_functions::matrix::{Budget, MatrixError, unit};
#[test]
fn identity_geometry_and_admission() {
    let b = Budget {
        max_cells: 9,
        max_work: 9,
    };
    let m = unit(3, b).unwrap();
    assert_eq!(m.values(), &[1., 0., 0., 0., 1., 0., 0., 0., 1.]);
    assert!(unit(0, b).is_err());
    assert_eq!(unit(4, b), Err(MatrixError::BudgetExceeded));
    assert_eq!(unit(usize::MAX, b), Err(MatrixError::SizeOverflow));
}
