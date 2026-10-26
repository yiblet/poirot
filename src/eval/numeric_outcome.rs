//! Numeric result selection is independent of text coercion and text semantics.
use crate::model::{CellError, CellValue};
/// Identifies the pinned upstream sources that define numeric result behavior.
/// Configuration fingerprints retain this compile-time binding so cached results
/// cannot silently cross numeric implementation changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NumericSourceBinding {
    /// Pinned Apache POI revision.
    pub poi_revision: &'static str,
    /// Digest of the pinned number wrapper source.
    pub number_eval_sha256: &'static str,
    /// Handler source identities included in this numeric contract.
    pub handler_sha256: &'static [(&'static str, &'static str)],
    /// Pinned Commons Math dependency version.
    pub commons_math_version: &'static str,
    /// Digest of the pinned Commons Math dependency.
    pub commons_math_sha256: &'static str,
}

/// Return the immutable source binding required by the pinned policy.
pub const fn source_binding() -> NumericSourceBinding {
    NumericSourceBinding {
        poi_revision: "732120980140d5ed64b482c470e0b625cdb1ab15",
        number_eval_sha256: "286f2c3303274e60b649e6d5090699c0b894ec7eb9d6cbe1fe1680ebe1b074e5",
        handler_sha256: &[
            (
                "IfFunc.java",
                "843c4bccc585e826e1d21454b7e54e0368b52d22b32622abc328b823523b4ed4",
            ),
            (
                "BooleanFunction.java",
                "1ccc8640525fb33347d8c95408a0901b0dd4b6927eaaec6234352fd4863ca290",
            ),
            (
                "Days360.java",
                "10c46d92db5a64b70dede0ea00348e896e6626f0b4499f10911126edad0c118c",
            ),
            (
                "LookupUtils.java",
                "3099062485d8cc6f04f98c3697750a2dee882c735ea05d09d0eaddedcb269e05",
            ),
            (
                "NormDist",
                "02c0d88d3063867b3aad33eb75b3cb241e4a55eafeefa54eb08d151b28a35779",
            ),
            (
                "NormSDist",
                "0a42c56598a3dd8e1a11f66317b5fba572ca7084e942732945a6b375f11d1deb",
            ),
            (
                "NumberToTextConverter",
                "03859f50e1b4fc9e48cc62f6db8fc79942b738da01d088405cf616073425983f",
            ),
            (
                "WorkbookEvaluator",
                "e399e6e602eb12031874ad593303a7158f1d62d503b78ca5dd90722d6022042d",
            ),
            (
                "MathX",
                "c05018df2216414042aebc615f3412826986f1de0afe102fe8b9a658e0b92064",
            ),
            (
                "ExcelArithmetic",
                "48606623bdbb84b07603134921615a795a87ff5b5b986bce5fed2110d9bc348f",
            ),
            (
                "NormInv",
                "bcd97f9915fc7914581b085a61bf13923e8ca02a092c6cb3ed28185de930a92d",
            ),
            (
                "NormSInv",
                "a2600b4e6f902d535ef1ff5357503ab5da2d58e8b041b2dc0755b91c0a609732",
            ),
            (
                "NumericFunction",
                "7466c67653f69b6f261d3dc726661446004be7aa72a946f218e7e833e774388c",
            ),
            (
                "MultiOperandNumericFunction",
                "a7f60188adeb512f3fe8445e75a34fb27ef5f2a388b5b96adac8c173c05e04b2",
            ),
            (
                "NumberComparer",
                "f5c63fa2f6c18d852267b2e70682fa1b10236a8574d667c4503a344f9d77cc89",
            ),
        ],
        commons_math_version: "3.6.1",
        commons_math_sha256: "1e56d7b058d28b65abd256b8458e3885b674c1d588fa43cd7d1cbb9c7ef2b308",
    }
}

/// Apply selected non-finite and subnormal number construction behavior.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn coerce_numeric_outcome(number: f64) -> CellValue {
    CellValue::Number(if number != 0.0 && number.abs() < f64::MIN_POSITIVE {
        0.0
    } else {
        number
    })
}
/// Construct a parsed numeric token, rejecting non-finite pinned literals.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn coerce_numeric_literal(number: f64) -> crate::eval::EvalResult<CellValue> {
    if !number.is_finite() {
        return Ok(CellValue::Error(CellError::Num));
    }
    // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
    Ok(coerce_numeric_outcome(number))
}
/// Require a finite result before applying policy-specific construction.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(in crate::eval) fn coerce_checked_numeric_result(number: f64) -> CellValue {
    if number.is_finite() {
        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
        coerce_numeric_outcome(number)
    } else {
        CellValue::Error(CellError::Num)
    }
}
