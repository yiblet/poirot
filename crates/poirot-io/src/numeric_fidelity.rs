//! File representation policy for numeric payloads.
use crate::{CellAddress, IeeeNumberBits, NumericFidelityError};

/// File format whose numeric representation is being checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericFileFormat {
    /// BIFF binary workbook representation.
    Xls,
    /// OOXML workbook representation.
    Xlsx,
}
/// Finite-only encoder contracts. These do not assert external application fidelity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FiniteNumericRepresentationPolicy {
    /// BIFF NUMBER and numeric FORMULA caches contain little-endian binary64.
    Binary64Finite,
    /// Worksheet decimal text must parse back to the identical binary64 bits.
    DecimalRoundTripFinite,
}
/// Worksheet name, cell address, and literal-versus-formula-cache location of a number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumericLocation {
    /// Worksheet name containing the numeric value.
    pub sheet: String,
    /// Coordinate containing the numeric value.
    pub address: CellAddress,
    /// Whether the value is a saved formula result.
    pub formula_cache: bool,
}
/// Why an exporter cannot preserve a numeric value under the selected file policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NumericFidelityReason {
    /// Target numeric contract rejects infinity or NaN.
    Nonfinite,
    /// Target decimal encoding cannot recover the original binary64 bits.
    DecimalRoundTripMismatch,
}

impl FiniteNumericRepresentationPolicy {
    /// Admit a number only when the target representation satisfies this policy.
    pub fn check(
        self,
        number: f64,
        format: NumericFileFormat,
        location: NumericLocation,
    ) -> Result<(), NumericFidelityError> {
        let bits = IeeeNumberBits::from_number(number);
        let reason = if !number.is_finite() {
            Some(NumericFidelityReason::Nonfinite)
        } else if self == Self::DecimalRoundTripFinite
            && !matches!(number.to_string().parse::<f64>(), Ok(value) if value.to_bits() == bits.bits())
        {
            Some(NumericFidelityReason::DecimalRoundTripMismatch)
        } else {
            None
        };
        if let Some(reason) = reason {
            Err(NumericFidelityError {
                bits,
                location,
                format,
                policy: self,
                reason,
            })
        } else {
            Ok(())
        }
    }
}
