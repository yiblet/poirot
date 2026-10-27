use super::compiled_formula::CompiledFormula;
use super::potential_reference::PotentialReference;
use super::preparation_policy::PreparationPolicy;
use super::scalar_locality::ScalarLocality;
use super::scalar_matcher::ScalarMatcher;
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Key scalar proofs by token structure and core literal kinds, validating varied coordinates.
pub(super) fn scalar_classification_key(
    source: &str,
    policy: &PreparationPolicy,
    key: &mut Vec<u8>,
    cells: &mut Vec<(usize, usize)>,
) -> Option<ScalarLocality> {
    key.clear();
    cells.clear();
    let mut point_cells = 0usize;
    let mut literal_values = (source.len() <= ScalarMatcher::MAX_SOURCE_BYTES)
        .then(std::collections::hash_map::DefaultHasher::new);
    for token in poirot_parsing::formula::classified::Tokens::new(source) {
        let Ok(token) = token else {
            return None;
        };
        use poirot_parsing::formula::classified::TokenKind;
        if matches!(token.kind, TokenKind::Cell(_)) {
            point_cells = point_cells.saturating_add(1);
            if point_cells > ScalarMatcher::MAX_CELLS {
                literal_values = None;
            }
            if literal_values.is_some() {
                if cells.try_reserve(1).is_ok() {
                    // SAFETY: Reservation admitted one additional canonical point span.
                    cells.push((token.start, token.end));
                } else {
                    literal_values = None;
                }
            }
        }
        // The key already retains every fixed token and cell placeholder. Only
        // value-erased literal spellings need a separate advisory hash stream.
        if matches!(token.kind, TokenKind::Text | TokenKind::Number)
            && let Some(values) = &mut literal_values
        {
            token.raw.hash(values);
        }
        let additional = match token.kind {
            TokenKind::Literal | TokenKind::FixedReference => token
                .raw
                .len()
                .checked_add(std::mem::size_of::<usize>())
                .and_then(|n| n.checked_add(1)),
            _ => Some(1),
        };
        let additional = additional?;
        if key.try_reserve(additional).is_err() {
            return None;
        }
        match token.kind {
            TokenKind::Text | TokenKind::Number => {
                // SAFETY: The reservation above admitted one additional byte.
                key.push(if token.kind == TokenKind::Text { 2 } else { 3 });
            }
            TokenKind::Literal | TokenKind::FixedReference => {
                // SAFETY: The reservation admitted tag + length word + raw bytes.
                key.push(0);
                // SAFETY: After the tag, the reserved length word and raw bytes remain.
                key.extend_from_slice(&token.raw.len().to_le_bytes());
                // SAFETY: After the tag and length word, raw.len() reserved bytes remain.
                key.extend_from_slice(token.raw.as_bytes());
            }
            TokenKind::Cell(cell) => {
                if cell.row >= policy.parse.max_rows || cell.column >= policy.parse.max_columns {
                    return None;
                }
                // SAFETY: The reservation above admitted one additional byte.
                key.push(1);
            }
        }
    }
    Some(literal_values.map_or(ScalarLocality::Ineligible, |values| {
        let mut locality = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut locality);
        values.finish().hash(&mut locality);
        ScalarLocality::Eligible(locality.finish())
    }))
}

pub(super) fn executable_hash(program: &crate::eval::compile::bytecode::Executable) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    program.template.operands.hash(&mut hash);
    program.template.aggregations.hash(&mut hash);
    program.template.reference_slots.hash(&mut hash);
    for instruction in &program.template.code {
        (
            instruction.opcode as u8,
            instruction.flags,
            instruction.reserved,
            instruction.a,
            instruction.b,
            instruction.c,
        )
            .hash(&mut hash);
    }
    hash.finish()
}

pub(super) fn classified_cell_count(source: &str, policy: &PreparationPolicy) -> Option<usize> {
    let mut count = 0;
    for token in poirot_parsing::formula::classified::Tokens::new(source) {
        if let poirot_parsing::formula::classified::TokenKind::Cell(cell) = token.ok()?.kind {
            if cell.row >= policy.parse.max_rows || cell.column >= policy.parse.max_columns {
                return None;
            }
            count += 1;
        }
    }
    Some(count)
}

// Keep formulas that rebound() must reject out of its candidate buckets. Otherwise
// a copied range-shaped formula adds another failed candidate on every compile.
pub(super) fn reusable_template(formula: &CompiledFormula) -> bool {
    classified_cell_count(&formula.source, &formula.policy)
        .is_some_and(|count| rebound(formula, &formula.source, count).is_some())
}

pub(super) fn rebound(
    exemplar: &CompiledFormula,
    source: &str,
    cell_count: usize,
) -> Option<CompiledFormula> {
    let program = &exemplar.bytecode;
    // Rebinding point cells preserves shape. Range endpoints can change a
    // producer's dimensions, so those bounds must be compiled again.
    if exemplar.properties().is_some_and(|p| p.result.may_spill())
        && poirot_parsing::formula::classified::Tokens::new(source)
            .any(|token| token.is_ok_and(|token| token.raw == ":"))
    {
        return None;
    }
    if program.references.slots.len() != cell_count
        || program
            .references
            .slots
            .iter()
            .any(|slot| slot.kind != crate::eval::compile::bytecode::ReferenceKind::Cell)
    {
        return None;
    }
    if exemplar.potential.references.len() != cell_count
        || exemplar
            .potential
            .references
            .iter()
            .any(|reference| !matches!(reference, PotentialReference::Cell(_)))
    {
        return None;
    }
    let mut references = program.references.clone();
    let mut potential = exemplar.potential.clone();
    let mut index = 0;
    for token in poirot_parsing::formula::classified::Tokens::new(source) {
        let poirot_parsing::formula::classified::TokenKind::Cell(cell) = token.ok()?.kind else {
            continue;
        };
        let slot = references.slots.get(index)?;
        for (word, value) in [
            cell.row,
            cell.column,
            cell.row_absolute.into(),
            cell.column_absolute.into(),
        ]
        .into_iter()
        .enumerate()
        {
            let offset = (slot.offset as usize).checked_add(word * 4)?;
            // SAFETY: The checked destination range has length four, matching the u32 byte array.
            references
                .data
                .get_mut(offset..offset.checked_add(4)?)?
                .copy_from_slice(&value.to_le_bytes());
        }
        let PotentialReference::Cell(reference) = potential
            .references
            .get_mut(cell_count.checked_sub(1)?.checked_sub(index)?)?
        else {
            return None;
        };
        reference.row = cell.row;
        reference.column = cell.column;
        reference.row_absolute = cell.row_absolute;
        reference.column_absolute = cell.column_absolute;
        index += 1;
    }
    Some(CompiledFormula {
        source: Arc::from(source),
        bytecode: crate::eval::compile::bytecode::Compiled {
            executable: Arc::clone(&program.executable),
            references,
        },
        potential,
        policy: exemplar.policy.clone(),
    })
}
