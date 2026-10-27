use crate::{
    FiniteNumericRepresentationPolicy, IeeeNumberBits, NumericFidelityReason, NumericFileFormat,
    NumericLocation,
};

/// Export diagnostic explaining why a number cannot satisfy the target file's representation policy.
/// It retains the exact bits and source location so callers can report the refusal
/// without turning it into a spreadsheet error value.
#[derive(Clone, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[error(
    "{format:?} numeric fidelity refusal at {location:?} (bits={bits:?}, policy={policy:?}, reason={reason:?})"
)]
pub struct NumericFidelityError {
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
