//! Independent examples of Microsoft's implicit-intersection rules:
//! https://support.microsoft.com/en-us/excel/implicit-intersection-operator
//! Singleton inputs survive; vectors select their unique matching row/column.
use poirot_functions_lookup::intersection_offsets;

#[test]
fn reference_intersection_is_unique_and_keeps_signed_origins() {
    for (rows, columns, at, expected) in [
        ((2, 2), (3, 3), (90, 90), Some((1, 1))),
        ((2, 5), (3, 3), (4, 90), Some((3, 1))),
        ((2, 2), (3, 7), (90, 5), Some((1, 3))),
        ((2, 5), (3, 7), (4, 5), None),
        ((2, 5), (3, 3), (90, 3), None),
        ((-2, 2), (-1, -1), (0, 90), Some((3, 1))),
        ((4, 2), (3, 3), (3, 3), None),
        ((2, 2), (4, 3), (2, 3), None),
        ((i64::MIN, i64::MAX), (0, 0), (i64::MAX, 0), None),
    ] {
        assert_eq!(intersection_offsets(rows, columns, at), expected);
    }
}
