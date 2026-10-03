//! Pure finite-only XLSX admission, shared by plan and serializer defenses.
use crate::IoError;
use poirot_model::{
    Cell, CellAddress, CellValue, FiniteNumericRepresentationPolicy, NumericFileFormat,
    NumericLocation,
};
/// Finite numeric representation contract enforced by this file adapter.
pub(super) const POLICY: FiniteNumericRepresentationPolicy =
    FiniteNumericRepresentationPolicy::DecimalRoundTripFinite;
/// Validate one source number before target-format serialization.
pub(super) fn check(sheet: &str, address: CellAddress, cell: &Cell) -> Result<(), IoError> {
    if let CellValue::Number(number) = &cell.value {
        POLICY
            .check(
                *number,
                NumericFileFormat::Xlsx,
                NumericLocation {
                    sheet: sheet.into(),
                    address,
                    formula_cache: cell.formula.is_some(),
                },
            )
            .map_err(IoError::NumericFidelity)?;
    }
    Ok(())
}
