//! Parser-independent, atomic application of source edit plans.
use crate::model::{CellAddress, CellValue, ModelError, Workbook, normalize_formula};

/// The location of a formula in a cell or defined name.
///
/// Indices refer to the workbook used to create the edit plan, not persistent IDs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormulaSource {
    /// Formula source stored at a worksheet coordinate.
    Cell {
        /// Zero-based worksheet index.
        sheet: usize,
        /// Formula cell coordinate.
        address: CellAddress,
    },
    /// Formula body stored as a defined name.
    DefinedName {
        /// Defined-name index in workbook source order.
        index: usize,
    },
}
/// A formula replacement that checks the existing text before applying a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormulaReplacement {
    /// Location of the cell or defined-name formula to update.
    pub source: FormulaSource,
    /// Exact source required before applying the replacement.
    pub expected: String,
    /// Translated formula spelling to store.
    pub replacement: String,
}
/// A validated sheet rename and the formula rewrites needed to preserve references.
/// Application rejects a changed source snapshot before mutating the workbook.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RenameSheetPlan {
    expected: Workbook,
    sheet: usize,
    name: String,
    replacements: Vec<FormulaReplacement>,
}
/// The renamed sheet and formula sources changed by a successful rename transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditOutcome {
    /// Index of the renamed worksheet.
    pub renamed_sheet: usize,
    /// Worksheet spelling before the edit.
    pub previous_name: String,
    /// Validated worksheet spelling after the edit.
    pub new_name: String,
    /// Locations of the cell and defined-name formulas rewritten by this edit.
    pub changed_sources: Vec<FormulaSource>,
}
impl RenameSheetPlan {
    /// Validate a parser-produced replacement set without mutating the workbook.
    pub(crate) fn new(
        workbook: &Workbook,
        sheet: usize,
        name: String,
        replacements: Vec<FormulaReplacement>,
    ) -> Result<Self, ModelError> {
        let mut validation = workbook.clone();
        validation.rename_sheet(sheet, &name)?;
        let mut seen = std::collections::BTreeSet::new();
        for edit in &replacements {
            if !seen.insert(edit.source.clone())
                || formula(workbook, &edit.source) != Some(edit.expected.as_str())
            {
                return Err(ModelError(
                    "invalid or duplicate formula replacement".into(),
                ));
            }
            if normalize_formula(edit.replacement.clone())? != edit.replacement {
                return Err(ModelError(
                    "replacement must be normalized formula source".into(),
                ));
            }
        }
        Ok(Self {
            expected: workbook.clone(),
            sheet,
            name,
            replacements,
        })
    }
    /// Borrow checked formula rewrites to apply atomically with the rename.
    #[cfg(test)]
    pub(crate) fn replacements(&self) -> &[FormulaReplacement] {
        &self.replacements
    }
}
fn formula<'a>(workbook: &'a Workbook, source: &FormulaSource) -> Option<&'a str> {
    match source {
        FormulaSource::Cell { sheet, address } => {
            workbook.sheet(*sheet)?.get(*address)?.formula.as_deref()
        }
        FormulaSource::DefinedName { index } => Some(&workbook.named_ranges.get(*index)?.formula),
    }
}
fn same_value(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Number(a), CellValue::Number(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}
// Direct mutable access can bypass revision tracking, so compare the formula
// inputs themselves. Stored formula results and styles do not affect a rename.
fn same_source(a: &Workbook, b: &Workbook) -> bool {
    a.date_system == b.date_system
        && a.named_ranges == b.named_ranges
        && a.sheets.len() == b.sheets.len()
        && a.sheets.iter().zip(&b.sheets).all(|(a, b)| {
            a.name == b.name
                && a.cells().len() == b.cells().len()
                && a.cells().iter().zip(b.cells()).all(|((aa, ac), (ba, bc))| {
                    aa == ba
                        && ac.formula == bc.formula
                        && ac.shared_formula == bc.shared_formula
                        && (ac.formula.is_some() || same_value(&ac.value, &bc.value))
                })
        })
}
impl Workbook {
    /// Apply a sheet rename and its formula replacements.
    ///
    /// Rejects a stale plan before changing the workbook. Stored results and styles
    /// are preserved.
    pub(crate) fn apply_rename_sheet_plan(
        &mut self,
        plan: &RenameSheetPlan,
    ) -> Result<EditOutcome, ModelError> {
        if !same_source(self, &plan.expected) {
            return Err(ModelError(
                "stale sheet rename plan: workbook source changed".into(),
            ));
        }
        let previous_name = self
            .sheets
            .get(plan.sheet)
            .ok_or_else(|| ModelError("sheet rename index out of bounds".into()))?
            .name
            .clone();
        // The source comparison above ensures that the indices and formula text
        // checked when the plan was built still refer to the same content.
        self.sheets
            .get_mut(plan.sheet)
            .ok_or_else(|| ModelError("sheet rename index out of bounds".into()))?
            .name
            .clone_from(&plan.name);
        for edit in &plan.replacements {
            match edit.source {
                FormulaSource::Cell { sheet, address } => {
                    let cell = self
                        .sheets
                        .get_mut(sheet)
                        .and_then(|worksheet| worksheet.cells.get_mut(&address))
                        .ok_or_else(|| ModelError("sheet rename cell missing".into()))?;
                    cell.formula = Some(edit.replacement.clone());
                    cell.shared_formula = None;
                }
                FormulaSource::DefinedName { index } => self
                    .named_ranges
                    .get_mut(index)
                    .ok_or_else(|| ModelError("sheet rename name missing".into()))?
                    .formula
                    .clone_from(&edit.replacement),
            }
        }
        Ok(EditOutcome {
            renamed_sheet: plan.sheet,
            previous_name,
            new_name: plan.name.clone(),
            changed_sources: plan.replacements.iter().map(|e| e.source.clone()).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Cell, SharedFormula, SharedFormulaId};
    fn at(text: &str) -> CellAddress {
        text.parse().unwrap()
    }
    fn shared() -> SharedFormula {
        SharedFormula {
            group: SharedFormulaId(7),
            origin: at("B1"),
        }
    }
    fn workbook() -> Workbook {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for (address, formula) in [("B1", "A1+1"), ("B2", "A2+1")] {
            let mut cell = Cell::with_formula(formula, CellValue::Number(2.)).unwrap();
            cell.shared_formula = Some(shared());
            sheet.set(at(address), cell).unwrap();
        }
        book
    }
    #[test]
    fn rename_formula_rewrites_detach_changed_sources_and_check_provenance() {
        let mut book = workbook();
        let edits = vec![FormulaReplacement {
            source: FormulaSource::Cell {
                sheet: 0,
                address: at("B1"),
            },
            expected: "A1+1".into(),
            replacement: "A1+2".into(),
        }];
        let plan = RenameSheetPlan::new(&book, 0, "Renamed".into(), edits).unwrap();
        let mut stale = book.clone();
        stale
            .sheet_mut(0)
            .unwrap()
            .get_mut(at("B2"))
            .unwrap()
            .shared_formula = None;
        let before = stale.clone();
        assert!(stale.apply_rename_sheet_plan(&plan).is_err());
        assert_eq!(stale, before);
        book.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            book.sheet(0).unwrap().get(at("B1")).unwrap().shared_formula,
            None
        );
        assert_eq!(
            book.sheet(0).unwrap().get(at("B2")).unwrap().shared_formula,
            Some(shared())
        );
    }
}
