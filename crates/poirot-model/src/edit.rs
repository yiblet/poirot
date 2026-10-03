//! Parser-independent, atomic application of source edit plans.
use crate::*;

/// Stable within an unchanged source snapshot; no persistent sheet IDs are implied.
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
/// One source-checked formula rewrite. `expected` must match the existing
/// normalized formula before `replacement` can be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaReplacement {
    /// Cell or defined-name identity to update.
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
    /// Number of cell/name formulas rewritten by this edit.
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
// Legacy mutable access precludes reliable revision counters. Compare actual source;
// saved formula results and styles are derived/presentation data, not formula source.
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
                        && (ac.formula.is_some() || same_value(&ac.value, &bc.value))
                })
        })
}
impl Workbook {
    /// Reject stale plans before any mutation. Preserve saved caches and styles.
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
        // All indices and replacement preconditions were validated during construction
        // and the source snapshot matches, so application has no fallible steps.
        self.sheets
            .get_mut(plan.sheet)
            .ok_or_else(|| ModelError("sheet rename index out of bounds".into()))?
            .name
            .clone_from(&plan.name);
        for edit in &plan.replacements {
            match edit.source {
                FormulaSource::Cell { sheet, address } => {
                    self.sheets
                        .get_mut(sheet)
                        .and_then(|worksheet| worksheet.cells.get_mut(&address))
                        .ok_or_else(|| ModelError("sheet rename cell missing".into()))?
                        .formula = Some(edit.replacement.clone())
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
