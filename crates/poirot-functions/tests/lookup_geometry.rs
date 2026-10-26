use poirot_functions::lookup::{Array, Error, Reference, ReferenceKind, Scalar, SignedReference};

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

#[test]
fn offset_axes_follow_documented_modern_grid_without_short_narrowing() {
    use poirot_functions::lookup::offset_extent;
    // Microsoft OFFSET: D3 displaced three rows and minus two columns is B6.
    // https://support.microsoft.com/en-us/excel/functions/offset-function
    assert_eq!(offset_extent(2, 3, 1, 1_048_576), Ok((5, 5)));
    assert_eq!(offset_extent(3, -2, 1, 16_384), Ok((1, 1)));
    for displacement in [32_767, 32_768, 65_535, 65_536, 1_048_575] {
        assert_eq!(
            offset_extent(0, displacement, 1, 1_048_576),
            Ok((displacement as u32, displacement as u32))
        );
    }
    assert_eq!(
        offset_extent(0, 0, 1_048_576, 1_048_576),
        Ok((0, 1_048_575))
    );
    assert_eq!(offset_extent(0, 16_383, 1, 16_384), Ok((16_383, 16_383)));
    for (start, delta, length, limit) in [
        (0, -1, 1, 1_048_576),
        (0, 1_048_576, 1, 1_048_576),
        (1_048_575, 0, 2, 1_048_576),
        (0, 16_384, 1, 16_384),
        (0, 0, 0, 1_048_576),
        (0, i64::MAX, 2, 1_048_576),
        (1, i64::MAX, 1, 1_048_576),
        (0, i64::MIN, -2, 1_048_576),
    ] {
        assert_eq!(offset_extent(start, delta, length, limit), Err(Error::Ref));
    }
}

#[test]
fn negative_offset_extents_normalize_without_original_endpoint_quirks() {
    // Retained compatibility policy: native Excel's negative-size behavior is unverified.
    use poirot_functions::lookup::offset_extent;
    assert_eq!(offset_extent(4, 0, -2, 1_048_576), Ok((3, 4)));
    assert_eq!(offset_extent(3, 0, -2, 16_384), Ok((2, 3)));
    assert_eq!(offset_extent(0, 0, -2, 1_048_576), Err(Error::Ref));
    assert_eq!(offset_extent(4, 0, i64::MIN, 1_048_576), Err(Error::Ref));
}
