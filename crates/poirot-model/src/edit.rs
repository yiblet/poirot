//! Parser-independent, atomic application of source edit plans.
use crate::*;

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
pub struct FormulaReplacement {
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
pub struct RenameSheetPlan {
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
    pub fn new(
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
    /// Return the worksheet index captured during edit planning.
    pub fn sheet(&self) -> usize {
        self.sheet
    }
    /// Borrow the validated destination sheet spelling.
    pub fn new_name(&self) -> &str {
        &self.name
    }
    /// Borrow checked formula rewrites to apply atomically with the rename.
    pub fn replacements(&self) -> &[FormulaReplacement] {
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
    pub fn apply_rename_sheet_plan(
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
