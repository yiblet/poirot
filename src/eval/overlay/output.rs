//! Shared output selection over the single execution engine.
use super::source;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{CapabilityId, EvaluationError, EvaluationReasonError};
use crate::eval::session::CellEvaluator;
use std::collections::BTreeMap;

/// Evaluate a caller-ordered list, retaining duplicates.
pub(super) fn outputs<const RETAINED: bool>(
    session: &mut crate::eval::session::Session<'_>,
    targets: &[EvaluationTarget],
) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationError> {
    session.state.runtime.budget.allocation(targets.len())?;
    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(targets.len())
        .map_err(|reservation_source| {
            EvaluationError::reservation(
                CapabilityId::FunctionAdapter,
                None,
                EvaluationReasonError::CellExecutionBookkeepingAllocationFailed,
                reservation_source,
            )
        })?;
    let mut evaluator = CellEvaluator::new(session);
    let mut remaining = targets;
    if RETAINED && let Some(cache) = evaluator.retained_output_cache() {
        remaining = project_cached_prefix(&mut outputs, remaining, |target| {
            evaluator.retained_output(&cache, target)
        })?;
    } else if !RETAINED && evaluator.has_local_output_cache() {
        remaining = project_cached_prefix(&mut outputs, remaining, |target| {
            evaluator.local_output(target)
        })?;
    }
    for &target in remaining {
        // SAFETY: The reservation covers both loops, and each target is visited once.
        outputs.push(evaluator.output(target)?);
    }
    Ok(outputs)
}

/// Consume the leading hits and leave later targets for the full execution path.
fn project_cached_prefix<'t>(
    outputs: &mut Vec<crate::eval::values::CellEvaluation>,
    mut targets: &'t [EvaluationTarget],
    mut read: impl FnMut(
        EvaluationTarget,
    )
        -> Option<crate::eval::limits::EvalResult<crate::eval::values::CellEvaluation>>,
) -> crate::eval::limits::EvalResult<&'t [EvaluationTarget]> {
    while let Some((&target, tail)) = targets.split_first() {
        let Some(output) = read(target) else {
            break;
        };
        // SAFETY: The caller reserved one output slot for each target in this batch.
        outputs.push(output?);
        targets = tail;
    }
    Ok(targets)
}

/// Evaluate sparse effective source cells and materialized spill children.
pub(super) fn all(
    session: &mut crate::eval::session::Session<'_>,
) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationError> {
    let targets = source::source_targets(session.sources.workbook, session.sources.inputs);
    session.state.runtime.budget.allocation(targets.len())?;
    let mut values = BTreeMap::new();
    for (sheet, address) in targets {
        values.insert(
            (sheet, address),
            CellEvaluator::new(session).cell(sheet, address)?,
        );
    }
    // Only generated spill children extend the sparse source view. Ordinary
    // dependency memo entries can be implicit blanks and vary with cache warmth.
    let publication = session.state.runtime.preparation.publication()?;
    for (&key, value) in &session.state.runtime.memo.cache {
        if publication.contains(key) {
            values.entry(key).or_insert_with(|| value.clone());
        }
    }
    session.state.runtime.budget.allocation(values.len())?;
    Ok(values
        .into_iter()
        .map(|((sheet, address), value)| {
            session
                .sources
                .output(EvaluationTarget { sheet, address }, value)
        })
        .collect())
}
