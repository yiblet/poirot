//! Explicit worksheet cache publication, independent of computed numeric results.
use crate::{EvaluationFailure, EvaluationTarget, NumericOutcomePolicy};
use poirot_model::{CellError, CellValue, IeeeNumberBits, IeeeNumberClass};
/// How a computed scalar is stored as a formula's saved result.
/// Default preserves the computed value; `PinnedPoiCellSetter` models the pinned
/// setter's normalization and requires the corresponding numeric outcome policy.
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
/// Pinned POI revision and source digests defining saved-result setter behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CellSetterSourceBinding {
    /// Pinned Apache POI revision.
    pub poi_revision: &'static str,
    /// Source digests defining the saved-result setter contract.
    pub source_sha256: &'static [(&'static str, &'static str)],
}
/// Saved-result publication policy and its optional pinned source binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormulaCacheMaterializationIdentity {
    /// Selected saved-result conversion policy.
    pub policy: FormulaCacheMaterializationPolicy,
    /// Pinned setter source identity, when selected.
    pub source: Option<CellSetterSourceBinding>,
}
/// Scalar type and numeric/boolean/error identity used in publication receipts.
/// Text is summarized by type rather than copied into each receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializedScalarSummary {
    /// Blank scalar.
    Blank,
    /// Exact IEEE numeric bits.
    Number(IeeeNumberBits),
    /// Text scalar without copying its contents.
    Text,
    /// Boolean scalar value.
    Boolean(bool),
    /// Spreadsheet error identity.
    Error(CellError),
}
impl MaterializedScalarSummary {
    /// Summarize a scalar's type and non-text identity without allocating.
    pub fn from_value(value: &CellValue) -> Self {
        match value {
            CellValue::Blank => Self::Blank,
            CellValue::Number(n) => Self::Number(IeeeNumberBits::from_number(*n)),
            CellValue::Text(_) | CellValue::TextUtf16(_) => Self::Text,
            CellValue::Boolean(b) => Self::Boolean(*b),
            CellValue::Error(e) => Self::Error(*e),
        }
    }
}
/// Evidence of one prepared formula-cache write: target, policy, computed/stored
/// scalar summaries, and whether the formula is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormulaCacheWriteReceipt {
    /// Formula cell receiving the saved result.
    pub target: EvaluationTarget,
    /// Conversion policy and pinned source identity used for publication.
    pub identity: FormulaCacheMaterializationIdentity,
    /// Raw computed scalar summary.
    pub computed: MaterializedScalarSummary,
    /// Scalar summary after saved-result conversion.
    pub stored: MaterializedScalarSummary,
    /// Whether publication preserves the authored formula.
    pub formula_retained: bool,
}
impl FormulaCacheMaterializationPolicy {
    /// Reject a setter policy incompatible with the selected numeric outcomes.
    pub fn validate(self, numeric: NumericOutcomePolicy) -> Result<(), EvaluationFailure> {
        if self == Self::PinnedPoiCellSetter && numeric != NumericOutcomePolicy::PinnedPoi7321209 {
            return Err(EvaluationFailure::IncompatibleMaterializationPolicy);
        }
        Ok(())
    }
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
    pub(crate) fn stored_summary(
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
    pub(crate) fn store(self, value: CellValue) -> CellValue {
        match self.stored_summary(MaterializedScalarSummary::from_value(&value)) {
            MaterializedScalarSummary::Error(e) => CellValue::Error(e),
            _ => value,
        }
    }
    /// Describe a prospective cache write without modifying the workbook.
    pub(crate) fn receipt(
        self,
        target: EvaluationTarget,
        value: &CellValue,
    ) -> FormulaCacheWriteReceipt {
        let computed = MaterializedScalarSummary::from_value(value);
        FormulaCacheWriteReceipt {
            target,
            identity: self.identity(),
            computed,
            stored: self.stored_summary(computed),
            formula_retained: true,
        }
    }
}

/// Per-formula write receipts from a successfully completed cache publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaCachePublicationReceipt {
    /// Policy shared by every write in this publication.
    pub identity: FormulaCacheMaterializationIdentity,
    /// Per-formula computed and stored result summaries.
    pub writes: Vec<FormulaCacheWriteReceipt>,
}
/// Evaluate all stored formula cells and stage their results without publishing them.
pub(crate) fn formula_updates(
    workbook: &poirot_model::Workbook,
    session: &mut crate::Session<'_>,
) -> crate::EvalResult<Vec<crate::CellEvaluation>> {
    let mut targets = Vec::new();
    for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
        session
            .budget
            .charge(crate::WorkKind::SparseScan, worksheet.cells().len() as u64)?;
        for (address, cell) in worksheet.cells() {
            if cell.formula.is_some() {
                session.budget.check_allocation(targets.len() + 1)?;
                session.budget.charge(crate::WorkKind::Materialization, 1)?;
                targets.push(crate::EvaluationTarget {
                    sheet,
                    address: *address,
                });
            }
        }
    }
    session.budget.allocation(targets.len())?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        results.push(crate::CellEvaluation {
            target,
            value: session.cell(target.sheet, target.address)?,
        });
    }
    Ok(results)
}
/// poirot atomic publication with typed computed/stored summaries. Not evaluateInCell.
/// Any typed abort leaves worksheet caches unchanged and returns no publication receipt.
pub fn run_recalculation_with_receipts(
    workbook: &mut poirot_model::Workbook,
    configuration: &crate::RunConfiguration,
    budgets: crate::EvaluationBudgets,
) -> crate::EvaluationOutcome<FormulaCachePublicationReceipt> {
    let prepared = crate::execute_configuration(workbook, configuration, budgets, |session| {
        let updates = formula_updates(workbook, session)?;
        session.budget.allocation(updates.len())?;
        let writes = updates
            .iter()
            .map(|update| {
                configuration
                    .materialization_policy()
                    .receipt(update.target, &update.value)
            })
            .collect();
        Ok((
            updates,
            FormulaCachePublicationReceipt {
                identity: configuration.materialization_policy().identity(),
                writes,
            },
        ))
    });
    match prepared {
        crate::EvaluationOutcome::Aborted { failure, usage } => {
            crate::EvaluationOutcome::Aborted { failure, usage }
        }
        crate::EvaluationOutcome::Complete {
            value: (updates, receipt),
            usage,
        } => {
            if updates.iter().any(|update| {
                workbook
                    .sheet(update.target.sheet)
                    .and_then(|sheet| sheet.get(update.target.address))
                    .is_none()
            }) {
                return crate::EvaluationOutcome::Aborted {
                    failure: crate::EvaluationFailure::StalePlan,
                    usage,
                };
            }
            for update in updates {
                let Some(cell) = workbook
                    .sheet_mut(update.target.sheet)
                    .and_then(|s| s.get_mut(update.target.address))
                else {
                    return crate::EvaluationOutcome::Aborted {
                        failure: crate::EvaluationFailure::StalePlan,
                        usage,
                    };
                };
                cell.value = configuration.materialization_policy().store(update.value);
            }
            crate::EvaluationOutcome::Complete {
                value: receipt,
                usage,
            }
        }
    }
}
