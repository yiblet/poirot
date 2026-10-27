//! Shared output selection over the single execution engine.
use super::source;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::eval::session::CellEvaluator;
use std::collections::BTreeMap;

/// Evaluate a caller-ordered list, retaining duplicates.
pub(super) fn outputs(
    session: &mut crate::eval::session::Session<'_>,
    targets: &[EvaluationTarget],
) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationError> {
    session.state.runtime.budget.allocation(targets.len())?;
    targets
        .iter()
        .map(|target| {
            CellEvaluator::new(session)
                .cell(target.sheet, target.address)
                .map(|value| session.sources.output(*target, value))
        })
        .collect()
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
