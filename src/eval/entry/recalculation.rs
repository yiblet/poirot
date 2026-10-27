//! Whole-workbook recalculation with atomic formula-cache publication.

use crate::eval::materialization::{
    FormulaCacheMaterializationPolicy, FormulaCachePublicationReceipt,
};
use crate::eval::session::CellEvaluator;
use crate::eval::values::CellEvaluation;

/// Evaluate all stored formula cells and stage their results without publishing them.
pub(in crate::eval) fn formula_updates(
    workbook: &crate::model::Workbook,
    session: &mut crate::eval::session::Session<'_>,
) -> crate::eval::limits::EvalResult<Vec<CellEvaluation>> {
    let mut targets = Vec::new();
    for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
        session.state.runtime.budget.charge(
            crate::eval::limits::WorkKind::SparseScan,
            worksheet.cells().len() as u64,
        )?;
        for (address, cell) in worksheet.cells() {
            if cell.formula.is_some() {
                session
                    .state
                    .runtime
                    .budget
                    .check_allocation(targets.len() + 1)?;
                session
                    .state
                    .runtime
                    .budget
                    .charge(crate::eval::limits::WorkKind::Materialization, 1)?;
                targets.push(crate::eval::coordinates::EvaluationTarget {
                    sheet,
                    address: *address,
                });
            }
        }
    }
    session.state.runtime.budget.allocation(targets.len())?;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        let value = CellEvaluator::new(session).cell(target.sheet, target.address)?;
        results.push(session.sources.output(target, value));
    }
    Ok(results)
}

/// poirot atomic publication with typed computed/stored summaries. Not evaluateInCell.
/// Any typed abort leaves worksheet caches unchanged and returns no publication receipt.
pub fn run_recalculation_with_receipts(
    workbook: &mut crate::model::Workbook,
    configuration: &crate::eval::config::RunConfiguration,
    budgets: crate::eval::limits::EvaluationBudgets,
) -> crate::eval::limits::EvaluationOutcome<FormulaCachePublicationReceipt> {
    match recalculate(workbook, configuration, budgets, true) {
        crate::eval::limits::EvaluationOutcome::Complete {
            value: (_, receipt),
            usage,
        } => crate::eval::limits::EvaluationOutcome::Complete {
            value: receipt,
            usage,
        },
        crate::eval::limits::EvaluationOutcome::Aborted { failure, usage } => {
            crate::eval::limits::EvaluationOutcome::Aborted { failure, usage }
        }
    }
}

/// Share atomic cache publication with optional per-formula receipts.
pub(in crate::eval) fn recalculate(
    workbook: &mut crate::model::Workbook,
    configuration: &crate::eval::config::RunConfiguration,
    budgets: crate::eval::limits::EvaluationBudgets,
    capture_receipts: bool,
) -> crate::eval::limits::EvaluationOutcome<(usize, FormulaCachePublicationReceipt)> {
    let prepared =
        super::direct_runs::execute_configuration(workbook, configuration, budgets, |session| {
            let updates = formula_updates(workbook, session)?;
            let changes = publication_changes(
                workbook,
                session,
                &updates,
                configuration.materialization_policy(),
            )?;
            if capture_receipts {
                session.state.runtime.budget.allocation(updates.len())?;
            }
            let writes = updates
                .iter()
                .filter(|_| capture_receipts)
                .map(|update| {
                    configuration
                        .materialization_policy()
                        .receipt(update.target, &update.value)
                })
                .collect();
            Ok((
                updates.len(),
                changes,
                FormulaCachePublicationReceipt {
                    identity: configuration.materialization_policy().identity(),
                    writes,
                },
            ))
        });
    match prepared {
        crate::eval::limits::EvaluationOutcome::Aborted { failure, usage } => {
            crate::eval::limits::EvaluationOutcome::Aborted { failure, usage }
        }
        crate::eval::limits::EvaluationOutcome::Complete {
            value: (count, changes, receipt),
            usage,
        } => {
            if workbook.apply(changes).is_err() {
                return crate::eval::limits::EvaluationOutcome::Aborted {
                    failure: crate::eval::limits::EvaluationFailure::StalePlan,
                    usage,
                };
            }
            crate::eval::limits::EvaluationOutcome::Complete {
                value: (count, receipt),
                usage,
            }
        }
    }
}

/// Stage formula and derived spill-cache writes for one atomic model publication.
fn publication_changes(
    workbook: &crate::model::Workbook,
    session: &mut crate::eval::session::Session<'_>,
    updates: &[CellEvaluation],
    policy: FormulaCacheMaterializationPolicy,
) -> crate::eval::limits::EvalResult<Vec<crate::model::WorkbookChange>> {
    use crate::model::WorkbookChange;
    let runtime = &mut *session.state.runtime;
    let publication = runtime.preparation.publication()?;
    let count = updates
        .len()
        .checked_mul(2)
        .and_then(|n| n.checked_add(publication.generated_count()))
        .ok_or(crate::eval::limits::EvaluationFailure::StalePlan)?;
    runtime.budget.allocation(count)?;
    let mut changes = Vec::with_capacity(count);
    for update in updates {
        changes.push(WorkbookChange::ClearSpillCache {
            sheet: update.target.sheet,
            anchor: update.target.address,
        });
    }
    for update in updates {
        let formula = workbook
            .sheet(update.target.sheet)
            .and_then(|sheet| sheet.source_cell(update.target.address))
            .and_then(|cell| cell.formula.as_deref())
            .ok_or(crate::eval::limits::EvaluationFailure::StalePlan)?;
        runtime.budget.text(formula.len())?;
        let value = policy.convert_stored_value(update.value.clone());
        if let Some(last) = publication.extent((update.target.sheet, update.target.address)) {
            changes.push(WorkbookChange::SetSpillAnchorCachedValue {
                sheet: update.target.sheet,
                anchor: update.target.address,
                last,
                expected_formula: formula.into(),
                value,
            });
        } else {
            changes.push(WorkbookChange::SetCachedValue {
                sheet: update.target.sheet,
                address: update.target.address,
                expected_formula: formula.into(),
                value,
            });
        }
    }
    let mut children: Vec<_> = publication.children().collect();
    children.sort_unstable();
    for ((sheet, address), anchor) in children {
        let formula = workbook
            .sheet(sheet)
            .and_then(|sheet| sheet.source_cell(anchor))
            .and_then(|cell| cell.formula.as_deref())
            .ok_or(crate::eval::limits::EvaluationFailure::StalePlan)?;
        let value = runtime
            .memo
            .cache
            .get(&(sheet, address))
            .ok_or(crate::eval::limits::EvaluationFailure::StalePlan)?;
        runtime
            .budget
            .text(formula.len().saturating_add(value.text_storage_bytes()))?;
        changes.push(WorkbookChange::SetSpillCachedValue {
            sheet,
            address,
            anchor,
            expected_formula: formula.into(),
            value: policy.convert_stored_value(value.clone()),
        });
    }
    Ok(changes)
}
