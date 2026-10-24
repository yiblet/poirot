//! Shared output selection over the single execution engine.
use super::*;

/// Evaluate a caller-ordered list, retaining duplicates.
pub(super) fn outputs(
    session: &mut crate::Session<'_>,
    targets: &[EvaluationTarget],
) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
    session.budget.allocation(targets.len())?;
    targets
        .iter()
        .map(|target| {
            session
                .cell(target.sheet, target.address)
                .map(|value| session.output(*target, value))
        })
        .collect()
}
/// Evaluate sparse effective source cells and materialized spill children.
pub(super) fn all(
    session: &mut crate::Session<'_>,
) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
    let targets = source::source_targets(session.workbook, session.inputs);
    session.budget.allocation(targets.len())?;
    let mut values = BTreeMap::new();
    for (sheet, address) in targets {
        values.insert((sheet, address), session.cell(sheet, address)?);
    }
    // Only generated spill children extend the sparse source view. Ordinary
    // dependency memo entries can be implicit blanks and vary with cache warmth.
    let publication = session.preparation.publication()?;
    for (&key, value) in &session.cache {
        if publication.contains(key) {
            values.entry(key).or_insert_with(|| value.clone());
        }
    }
    session.budget.allocation(values.len())?;
    Ok(values
        .into_iter()
        .map(|((sheet, address), value)| session.output(EvaluationTarget { sheet, address }, value))
        .collect())
}
