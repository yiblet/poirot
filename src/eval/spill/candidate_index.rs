use super::discovery;
use super::sheet_candidates::SheetCandidates;
use crate::eval::compile::SourcePreparation;
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::functions::extensions;
use crate::eval::limits::{BudgetMeter, EvalResult, EvaluationBudgets, EvaluationError};
use crate::model::{CellAddress, Workbook};

/// Worksheet source-inspection proofs and possible rectangles under one preparation identity.
pub(in crate::eval) type CandidateIndex = std::collections::BTreeMap<usize, SheetCandidates>;

/// Capture the bounded source prefixes whose candidate discovery has completed.
pub(in crate::eval) fn inspected_prefixes(index: &CandidateIndex) -> Vec<(usize, CellAddress)> {
    index
        .iter()
        .flat_map(|(&sheet, discovery)| {
            discovery
                .inspected
                .iter()
                .map(move |(&col, &row)| (sheet, CellAddress { row, col }))
        })
        .collect()
}

/// Reinspect saved source coverage without calculating formulas or trusting persisted proofs.
pub(in crate::eval) fn restore_candidates(
    workbook: &Workbook,
    inputs: &crate::eval::inputs::RunInputs,
    compiler: &std::cell::RefCell<FormulaCompiler>,
    policy: &PreparationPolicy,
    extensions: Option<&extensions::ExtensionRegistry>,
    prefixes: &[(usize, CellAddress)],
) -> EvalResult<CandidateIndex> {
    let mut index = CandidateIndex::default();
    let mut budget = BudgetMeter::configured(EvaluationBudgets::default(), false)?;
    for &(sheet, last) in prefixes {
        if workbook.sheet(sheet).is_none() {
            return Err(EvaluationError::StalePlan);
        }
        discovery::Discovery {
            workbook,
            inputs: Some(inputs),
            plan: None,
            source_preparation: SourcePreparation {
                compiler: Some(compiler),
                policy: Some(policy),
                extensions,
            },
            shared: None,
            local: &mut index,
            budget: &mut budget,
            consumer: None,
        }
        .discover(sheet, last)?;
    }
    Ok(index)
}
