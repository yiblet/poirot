//! Identify formula checks from the original Function / Expected / Correct layout.
use crate::fods::xml::model::{CellInput, FormulaCase, Omission, Sheet};
use std::collections::BTreeMap;

/// Render a zero-based native fixture coordinate as A1 text for case identities.
pub(crate) fn address(row: u32, col: u32) -> String {
    let mut col = col + 1;
    let mut letters = Vec::new();
    while col > 0 {
        letters.push((b'A' + ((col - 1) % 26) as u8) as char);
        col = (col - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", letters.into_iter().collect::<String>(), row + 1)
}
fn omission(sheet: &Sheet, at: Option<(u32, u32)>, reason: &str) -> Omission {
    Omission {
        kind: crate::fods::xml::model::OmissionKind::UnsupportedConstruct,
        source: None,
        sheet: Some(sheet.name.clone()),
        cell: at.map(|(r, c)| address(r, c)),
        reason: reason.into(),
    }
}
/// Derive a stable filesystem-safe fixture key from its pinned source path.
pub(crate) fn source_key(source: &str) -> String {
    let key = source
        .strip_prefix("sc/qa/unit/data/functions/")
        .unwrap_or(source);
    let key = key.strip_suffix(".fods").unwrap_or(key).replace('/', "__");
    key.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
/// Extract source checks, retaining their independent Expected cells and predicates.
pub fn extract(
    sheets: &[Sheet],
    source: &str,
) -> Result<(Vec<FormulaCase>, Vec<Omission>), String> {
    let mut cases = Vec::new();
    let mut omissions = Vec::new();
    for (sheet_index, sheet) in sheets.iter().enumerate() {
        let cells: BTreeMap<(u32, u32), &CellInput> =
            sheet.cells.iter().map(|c| ((c.row, c.col), c)).collect();
        let headers: BTreeMap<&str, u32> = cells
            .values()
            .filter(|c| c.row == 0)
            .filter_map(|c| c.value.as_text().map(|s| (s, c.col)))
            .collect();
        let (Some(&expected), Some(&correct)) = (headers.get("Expected"), headers.get("Correct"))
        else {
            continue;
        };
        if expected == 0 || expected.checked_mul(2) != Some(correct) {
            omissions.push(omission(
                sheet,
                None,
                "nonstandard Expected/Correct column layout",
            ));
            continue;
        }
        for (&(row, col), checker) in &cells {
            if row == 0 || col != correct {
                continue;
            }
            let Some(checker_formula) = &checker.formula else {
                omissions.push(omission(
                    sheet,
                    Some((row, col)),
                    "Correct cell is not a formula",
                ));
                continue;
            };
            for offset in 0..expected {
                let target = cells.get(&(row, offset));
                let Some(target) = target.filter(|c| c.formula.is_some() || c.matrix.is_some())
                else {
                    omissions.push(omission(
                        sheet,
                        Some((row, offset)),
                        "target is an array continuation or constant, not an explicit formula",
                    ));
                    continue;
                };
                let expected_cell = cells
                    .get(&(row, expected + offset))
                    .map(|c| (*c).clone())
                    .unwrap_or_else(|| CellInput::blank(row, expected + offset));
                cases.push(FormulaCase {
                    id: String::new(),
                    sheet: sheet_index,
                    sheet_name: sheet.name.clone(),
                    row,
                    col: offset,
                    cell: address(row, offset),
                    formula: target
                        .formula
                        .clone()
                        .unwrap_or_else(|| address(row, offset)),
                    original_formula: target
                        .original_formula
                        .clone()
                        .ok_or("source target formula has no original expression")?,
                    expected: expected_cell,
                    correct_row: row,
                    correct_col: correct,
                    correct_formula: checker_formula.clone(),
                    original_correct_formula: checker
                        .original_formula
                        .clone()
                        .ok_or("source Correct formula has no original expression")?,
                    matrix: target.matrix.clone(),
                });
            }
        }
    }
    let key = source_key(source);
    for (index, case) in cases.iter_mut().enumerate() {
        case.id = format!("{key}__{index:05}");
    }
    Ok((cases, omissions))
}
