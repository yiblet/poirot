use crate::eval::values::area_kind::AreaKind;
use crate::eval::values::signed_reference::grid_read::GridRead;
use crate::eval::values::signed_reference::signed_area::SignedArea;
use crate::model::CellAddress;

#[test]
fn logical_shape_and_sparse_grid_projection_remain_distinct() {
    let area = SignedArea::new(AreaKind::Area, 2, (-1, 2), (-2, 1)).unwrap();
    assert_eq!((area.height(), area.width()), (4, 4));
    let physical = area.physical().unwrap();
    assert_eq!(
        (
            physical.sheet,
            physical.r0,
            physical.r1,
            physical.c0,
            physical.c1
        ),
        (2, 0, 2, 0, 1)
    );
    assert_eq!(area.read(0, 3), Some(GridRead::Blank));
    assert_eq!(area.read(1, 0), Some(GridRead::Blank));
    assert_eq!(
        area.read(1, 2),
        Some(GridRead::Cell {
            sheet: 2,
            address: CellAddress { row: 0, col: 0 }
        })
    );
    assert_eq!(
        area.read(3, 3),
        Some(GridRead::Cell {
            sheet: 2,
            address: CellAddress { row: 2, col: 1 }
        })
    );
    assert_eq!(area.read(4, 0), None);
}

#[test]
fn wholly_off_grid_area_keeps_nonempty_logical_shape() {
    let area = SignedArea::new(AreaKind::Area, 0, (-2, -1), (-3, -1)).unwrap();
    assert_eq!((area.height(), area.width()), (2, 3));
    assert!(area.physical().is_none());
    assert_eq!(area.read(1, 2), Some(GridRead::Blank));
    assert!(SignedArea::new(AreaKind::Area, 0, (2, 1), (0, 0)).is_none());
}
