use crate::eval::reuse::Key;
use crate::model::{Cell, CellStyle, CellValue, WorkbookChange};

pub(super) fn same_stored_value(a: &CellValue, b: &CellValue) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
        && crate::eval::ports::same_result_identity(a, b)
}

pub(super) fn same_cell(a: &Cell, b: &Cell) -> bool {
    a.formula == b.formula
        && a.shared_formula == b.shared_formula
        && a.style == b.style
        && same_stored_value(&a.value, &b.value)
}

pub(super) fn append_cell_changes(
    changes: &mut Vec<WorkbookChange>,
    (sheet, address): Key,
    original: Option<&Cell>,
    cell: &Cell,
) {
    // A same-text source replacement can still detach shared membership. Publish
    // the complete record so cache/style-only changes retain the original metadata.
    if original.and_then(|c| c.shared_formula) != cell.shared_formula {
        changes.push(WorkbookChange::SetCell {
            sheet,
            address,
            cell: cell.clone(),
        });
        return;
    }
    let formula_changed = original.and_then(|c| c.formula.as_deref()) != cell.formula.as_deref();
    if formula_changed && let Some(formula) = &cell.formula {
        changes.push(WorkbookChange::SetFormula {
            sheet,
            address,
            formula: formula.clone(),
        });
    }
    if cell.formula.is_none() {
        if original.is_none_or(|c| c.formula.is_some() || !same_stored_value(&c.value, &cell.value))
        {
            changes.push(WorkbookChange::SetValue {
                sheet,
                address,
                value: cell.value.clone(),
            });
        }
    } else if (!formula_changed
        && original.is_none_or(|c| !same_stored_value(&c.value, &cell.value)))
        || (formula_changed && !matches!(cell.value, CellValue::Blank))
    {
        changes.push(WorkbookChange::SetCachedValue {
            sheet,
            address,
            // SAFETY: The preceding if handles formula.is_none(); this else branch therefore has Some, and formula is not mutated here.
            expected_formula: cell.formula.clone().unwrap(),
            value: cell.value.clone(),
        });
    }
    if original.map(|c| &c.style).unwrap_or(&CellStyle::default()) != &cell.style {
        changes.push(WorkbookChange::SetStyle {
            sheet,
            address,
            style: cell.style.clone(),
        });
    }
}

pub(super) fn result_change(
    source: Option<&Cell>,
    (sheet, address): Key,
    value: &CellValue,
    materialization: crate::eval::materialization::FormulaCacheMaterializationPolicy,
) -> Option<WorkbookChange> {
    // An implicit blank is an evaluation value, not authored source. Reads may
    // memoize it, but only source edits or spill publication can create a cell.
    // Authored blank edits are emitted separately by source_changes; formula
    // cells remain present here and publish their completed worksheet result.
    if source.is_none() && matches!(value, CellValue::Blank) {
        return None;
    }
    // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
    let stored = materialization.convert_stored_value(value.clone());
    if source.is_some_and(|c| same_stored_value(&c.value, &stored)) {
        return None;
    }
    Some(match source.and_then(|c| c.formula.as_ref()) {
        Some(formula) => WorkbookChange::SetCachedValue {
            sheet,
            address,
            expected_formula: formula.clone(),
            value: stored,
        },
        None => WorkbookChange::SetValue {
            sheet,
            address,
            value: stored,
        },
    })
}
