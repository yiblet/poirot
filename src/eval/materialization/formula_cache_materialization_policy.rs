//! How computed scalars are stored as saved formula results.

use super::cell_setter_source_binding::CellSetterSourceBinding;
use super::formula_cache_materialization_identity::FormulaCacheMaterializationIdentity;
use super::formula_cache_write_receipt::FormulaCacheWriteReceipt;
use super::materialized_scalar_summary::MaterializedScalarSummary;
use crate::eval::coordinates::EvaluationTarget;
use crate::model::{CellError, CellValue, IeeeNumberClass};

/// How a computed scalar is stored as a formula's saved result.
/// Default preserves the computed value; `PinnedPoiCellSetter` models the pinned
/// setter's normalization.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FormulaCacheMaterializationPolicy {
    /// Save the computed scalar unchanged.
    #[default]
    PreserveComputed,
    /// Apply the pinned POI cell setter's numeric normalization.
    PinnedPoiCellSetter,
}

impl FormulaCacheMaterializationPolicy {
    /// Return the immutable pinned setter source contract, when selected.
    pub const fn source_binding(self) -> Option<CellSetterSourceBinding> {
        match self {
            Self::PreserveComputed => None,
            Self::PinnedPoiCellSetter => Some(CellSetterSourceBinding {
                poi_revision: "732120980140d5ed64b482c470e0b625cdb1ab15",
                source_sha256: &[
                    (
                        "BaseFormulaEvaluator",
                        "e7dd8ca5722a98e10680c02458e35dc67926d200f59451a83d0cb83775c19c51",
                    ),
                    (
                        "CellBase",
                        "b4663e1f452c15eec9a436c81ef62938d45406318b0c5f58cd8f189b80306a3b",
                    ),
                    (
                        "CellValue",
                        "ca88d63d3a0f7c7bff71d71fb52c60302e791198caa616dd2eff093b44210b58",
                    ),
                    (
                        "HSSFFormulaEvaluator",
                        "d26829f9d518fee4f6f8aec697f285d9fa093a4b92c1b8c2c94d63eaafbfde76",
                    ),
                    (
                        "HSSFCell",
                        "da0c3d0f9ffd646038a0eb6b914c48fb21967cbaeeb2fdeb79f0cf31a5b6314f",
                    ),
                    (
                        "BaseXSSFFormulaEvaluator",
                        "9d48a957d8cbaeb480558b28bd0aa0c4b094d406c673673ff600385df651e6a7",
                    ),
                    (
                        "XSSFFormulaEvaluator",
                        "4dea74e6a722f29ed5b44963a715fb1335ccd335122ed35d132bc0a6e4f4b218",
                    ),
                    (
                        "XSSFCell",
                        "549cf3ab6db15eabdea2fceee4b51c61672aa81b4cb72c17b14e625132024134",
                    ),
                ],
            }),
        }
    }

    /// Capture the exact publication policy and source identity.
    pub const fn identity(self) -> FormulaCacheMaterializationIdentity {
        FormulaCacheMaterializationIdentity {
            policy: self,
            source: self.source_binding(),
        }
    }

    /// Predict saved-result normalization from a scalar summary.
    // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
    pub(in crate::eval) fn convert_stored_summary(
        self,
        computed: MaterializedScalarSummary,
    ) -> MaterializedScalarSummary {
        if self == Self::PinnedPoiCellSetter
            && let MaterializedScalarSummary::Number(bits) = computed
        {
            match bits.class() {
                IeeeNumberClass::PositiveInfinity | IeeeNumberClass::NegativeInfinity => {
                    return MaterializedScalarSummary::Error(CellError::Div0);
                }
                IeeeNumberClass::Nan => return MaterializedScalarSummary::Error(CellError::Num),
                _ => {}
            }
        }
        computed
    }

    /// Apply only at prepared formula-cache writes; ordinary setters/imports unchanged.
    // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
    pub(in crate::eval) fn convert_stored_value(self, value: CellValue) -> CellValue {
        // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
        match self.convert_stored_summary(MaterializedScalarSummary::from_value(&value)) {
            MaterializedScalarSummary::Error(e) => CellValue::Error(e),
            _ => value,
        }
    }

    /// Describe a prospective cache write without modifying the workbook.
    pub(in crate::eval) fn receipt(
        self,
        target: EvaluationTarget,
        value: &CellValue,
    ) -> FormulaCacheWriteReceipt {
        let computed = MaterializedScalarSummary::from_value(value);
        FormulaCacheWriteReceipt {
            target,
            identity: self.identity(),
            computed,
            // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
            stored: self.convert_stored_summary(computed),
            formula_retained: true,
        }
    }
}
