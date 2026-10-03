//! File representation policy, independent from evaluation/source exceptions.
use crate::{CellAddress, IeeeNumberBits};
use std::fmt;

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
    /// Zero-based worksheet index containing the numeric value.
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
/// Operational refusal, never an Excel NUM error or a pinned source exception.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NumericFidelityRefusal {
    /// Exact numeric payload refused by the target policy.
    pub bits: IeeeNumberBits,
    /// Source coordinate and literal/cache provenance.
    pub location: NumericLocation,
    /// Target workbook file format.
    pub format: NumericFileFormat,
    /// Representation contract that refused the value.
    pub policy: FiniteNumericRepresentationPolicy,
    /// Specific representation failure.
    pub reason: NumericFidelityReason,
}
impl FiniteNumericRepresentationPolicy {
    /// Admit a number only when the target representation satisfies this policy.
    pub fn check(
        self,
        number: f64,
        format: NumericFileFormat,
        location: NumericLocation,
    ) -> Result<(), NumericFidelityRefusal> {
        let bits = IeeeNumberBits::from_number(number);
        let reason = if !number.is_finite() {
            Some(NumericFidelityReason::Nonfinite)
        } else if self == Self::DecimalRoundTripFinite
            && number.to_string().parse::<f64>().ok().map(f64::to_bits) != Some(bits.bits())
        {
            Some(NumericFidelityReason::DecimalRoundTripMismatch)
        } else {
            None
        };
        if let Some(reason) = reason {
            Err(NumericFidelityRefusal {
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
impl fmt::Display for NumericFidelityRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} numeric fidelity refusal at {}!{} (formula_cache={}, bits={:016x}, policy={:?}, reason={:?})",
            self.format,
            self.location.sheet,
            self.location.address,
            self.location.formula_cache,
            self.bits.bits(),
            self.policy,
            self.reason
        )
    }
}
impl std::error::Error for NumericFidelityRefusal {}
