use poirot_functions_lookup::{Array, Error, Reference, ReferenceKind, Scalar, SignedReference};

#[test]
fn ordinary_reference_rejects_reversed_and_off_grid_bounds() {
    // Constructor contracts use inclusive, ordered, zero-based Excel worksheet bounds.
    for (sheets, rows, cols) in [
        (std::ops::RangeInclusive::new(2, 1), 0..=0, 0..=0),
        (0..=0, std::ops::RangeInclusive::new(1, 0), 0..=0),
        (0..=0, 0..=0, std::ops::RangeInclusive::new(1, 0)),
        (0..=0, 0..=1_048_576, 0..=0),
        (0..=0, 0..=0, 0..=16_384),
        (0..=0, 0..=u32::MAX, 0..=0),
        (0..=0, 0..=0, 0..=u32::MAX),
    ] {
        assert_eq!(
            Reference::new(ReferenceKind::Area, None, sheets, rows, cols),
            Err(Error::Ref.into())
        );
    }
}

#[test]
fn ordinary_reference_retains_identity_and_full_grid_dimensions() {
    let reference = Reference::new(
        ReferenceKind::Area,
        Some("book.xlsx".into()),
        0..=u32::MAX,
        0..=1_048_575,
        0..=16_383,
    )
    .unwrap();
    assert_eq!(reference.kind(), ReferenceKind::Area);
    assert_eq!(reference.workbook(), Some("book.xlsx"));
    assert_eq!(
        (reference.first_sheet(), reference.last_sheet()),
        (0, u32::MAX)
    );
    assert_eq!(
        (reference.first_row(), reference.last_row()),
        (0, 1_048_575)
    );
    assert_eq!((reference.first_col(), reference.last_col()), (0, 16_383));
    assert_eq!((reference.height(), reference.width()), (1_048_576, 16_384));
    for kind in [ReferenceKind::Cell, ReferenceKind::Area] {
        let reference =
            Reference::new(kind, None, 0..=0, 1_048_575..=1_048_575, 16_383..=16_383).unwrap();
        assert_eq!(reference.kind(), kind);
        assert_eq!((reference.height(), reference.width()), (1, 1));
    }
}

#[test]
fn replacing_array_members_preserves_shape_and_row_major_order() {
    let mut array = Array::new(2, 1, vec![Scalar::Blank, Scalar::Number(7.)]).unwrap();
    array.values_mut()[0] = Scalar::Missing;
    assert_eq!((array.rows(), array.cols()), (2, 1));
    assert_eq!(array.values(), &[Scalar::Missing, Scalar::Number(7.)]);
    assert_eq!(
        array.into_values(),
        vec![Scalar::Missing, Scalar::Number(7.)]
    );
}

#[test]
fn signed_source_geometry_remains_independent_of_ordinary_validation() {
    // Signed source endpoints intentionally retain off-grid and reversed geometry.
    let reference = SignedReference {
        kind: ReferenceKind::Area,
        workbook: None,
        first_sheet: 0,
        last_sheet: 0,
        first_row: -2,
        last_row: 1,
        first_col: 2,
        last_col: 0,
    };
    assert!(reference.valid());
    assert_eq!(
        (reference.source_height(), reference.source_width()),
        (4, -1)
    );
    assert_eq!((reference.height(), reference.width()), (4, 0));
}
