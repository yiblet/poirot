//! Public decimal rounding range and overflow contracts.

use poirot_functions::{math, scalar::CellError};

#[test]
fn rounding_limits_are_spreadsheet_errors() {
    // rust_decimal's documented coefficient is at most 2^96-1, with scale 0..=28.
    // https://docs.rs/rust_decimal/1.43.0/rust_decimal/struct.Decimal.html
    for value in [1e29, -1e29, 1e-29, -1e-29] {
        assert_eq!(math::round(value, 2.0), Err(CellError::Num));
        assert_eq!(math::round_up(value, 2.0), Err(CellError::Num));
        assert_eq!(math::round_down(value, 2.0), Err(CellError::Num));
        assert_eq!(math::ceiling_precise(value, 0.01), Err(CellError::Num));
        assert_eq!(math::floor_precise(value, 0.01), Err(CellError::Num));
        assert_eq!(
            math::mround(value, 0.01_f64.copysign(value)),
            Err(CellError::Num)
        );
    }
    assert_eq!(math::round(1e-28, 28.0), Ok(1e-28));
    assert_eq!(math::round(1e28, -28.0), Ok(1e28));
    assert_eq!(math::round(7.9e28, -28.0), Err(CellError::Num));
    assert_eq!(math::round_down(7.9e28, -28.0), Ok(7e28));
    assert_eq!(math::ceiling_precise(1e-28, 10.0), Ok(10.0));
    assert_eq!(math::floor_precise(-1e-28, 10.0), Ok(-10.0));
    assert_eq!(math::round_up(1e-28, -1.0), Ok(10.0));
    assert_eq!(math::mround(1.0, 0.0), Ok(0.0));
}
