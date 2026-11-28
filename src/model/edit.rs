//! Parser-independent, atomic application of source edit plans.
use crate::model::{CellAddress, CellValue, ModelError, Workbook, normalize_formula};

/// The location of a stored workbook formula.
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
    /// Formula definition used to fill a worksheet table column.
    CalculatedColumn {
        /// Zero-based worksheet index.
        sheet: usize,
        /// Table index in worksheet source order.
        table: usize,
        /// Column index in table order.
        column: usize,
    },
}

impl FormulaSource {
    /// Borrow the formula stored at this source, when present.
    pub(crate) fn formula<'a>(&self, workbook: &'a Workbook) -> Option<&'a str> {
        match *self {
            Self::Cell { sheet, address } => workbook
                .sheet(sheet)?
                .get_validated(address)?
                .formula
                .as_deref(),
            Self::DefinedName { index } => Some(&workbook.named_ranges.get(index)?.formula),
            Self::CalculatedColumn {
                sheet,
                table,
                column,
            } => Some(
                &workbook
                    .sheet(sheet)?
                    .tables()
                    .get(table)?
                    .columns
                    .get(column)?
                    .calculated_formula
                    .as_ref()?
                    .text,
            ),
        }
    }
}

/// A formula replacement that checks the existing text before applying a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormulaReplacement {
    /// Location of the formula to update.
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
    pub(super) expected: Workbook,
    pub(super) sheet: usize,
    pub(super) name: String,
    pub(super) replacements: Vec<FormulaReplacement>,
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
    /// Locations of the formulas rewritten by this edit.
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
        validation.rename_sheet_metadata(sheet, &name)?;
        let mut seen = std::collections::BTreeSet::new();
        for edit in &replacements {
            if !seen.insert(edit.source.clone())
                || edit.source.formula(workbook) != Some(edit.expected.as_str())
            {
                return Err(ModelError::InvalidFormulaReplacement {
                    target: edit.source.clone(),
                });
            }
            if normalize_formula(edit.replacement.clone())? != edit.replacement {
                return Err(ModelError::UnnormalizedFormulaReplacement {
                    target: edit.source.clone(),
                });
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
fn same_value(a: &CellValue, b: &CellValue) -> bool {
    match (a, b) {
        (CellValue::Number(a), CellValue::Number(b)) => a.to_bits() == b.to_bits(),
        _ => a == b,
    }
}
// Direct mutable access can bypass revision tracking, so compare the formula
// inputs themselves. Stored formula results and styles do not affect a rename.
pub(super) fn same_source(a: &Workbook, b: &Workbook) -> bool {
    a.date_system == b.date_system
        && a.named_ranges == b.named_ranges
        && a.sheets.len() == b.sheets.len()
        && a.sheets.iter().zip(&b.sheets).all(|(a, b)| {
            a.name == b.name
                && a.tables() == b.tables()
                && a.cells().len() == b.cells().len()
                && a.cells().iter().zip(b.cells()).all(|((aa, ac), (ba, bc))| {
                    aa == ba
                        && ac.formula == bc.formula
                        && ac.shared_formula == bc.shared_formula
                        && (ac.formula.is_some() || same_value(&ac.value, &bc.value))
                })
        })
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
            .unwrap()
            .shared_formula = None;
        let before = stale.clone();
        assert!(stale.apply_rename_sheet_plan(&plan).is_err());
        assert_eq!(stale, before);
        book.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            book.sheet(0)
                .unwrap()
                .get(at("B1"))
                .unwrap()
                .unwrap()
                .shared_formula,
            None
        );
        assert_eq!(
            book.sheet(0)
                .unwrap()
                .get(at("B2"))
                .unwrap()
                .unwrap()
                .shared_formula,
            Some(shared())
        );
    }
}
