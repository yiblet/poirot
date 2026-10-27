use crate::model::{CellAddress, Workbook};
use std::hash::Hash;

/// Exact formula, name, sheet, and visibility metadata guarding workbook plan reuse.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(super) struct SourceIdentity {
    pub(super) sheets: Vec<String>,
    pub(super) visibility: Vec<(usize, u32, crate::model::RowVisibility)>,
    pub(super) formulas: Vec<(usize, CellAddress, String)>,
    pub(super) names: Vec<(String, String, Option<usize>)>,
}

impl SourceIdentity {
    pub(super) fn capture(workbook: &Workbook) -> Self {
        let mut formulas = Vec::new();
        for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
            for (address, cell) in worksheet.cells() {
                if let Some(source) = &cell.formula {
                    formulas.push((sheet, *address, source.clone()));
                }
            }
        }
        formulas.sort_by_key(|(sheet, at, _)| (*sheet, at.row, at.col));
        Self {
            sheets: workbook
                .sheets()
                .iter()
                .map(|s| s.name().to_owned())
                .collect(),
            formulas,
            visibility: workbook
                .sheets()
                .iter()
                .enumerate()
                .flat_map(|(sheet, worksheet)| {
                    worksheet
                        .row_visibility_entries()
                        .map(move |(row, flags)| (sheet, row, flags))
                })
                .collect(),
            names: workbook
                .named_ranges
                .iter()
                .map(|n| (n.name.clone(), n.formula.clone(), n.sheet_scope))
                .collect(),
        }
    }
}
