//! Formula-aware structural edits: pure planning, then atomic model application.
use crate::model::{
    EditOutcome, FormulaReplacement, FormulaSource, ModelError, RenameSheetPlan, Workbook,
};
use poirot_parsing::formula::{
    Expr, FormulaParsePolicy, FormulaPrinter, ParseError, parse_formula_with_policy,
};
use std::fmt;

/// A workbook edit rejected by model validation, identifier comparison, or formula parsing.
/// The formula source is retained when a particular cell or defined name caused failure.
#[derive(Debug)]
pub enum EditError {
    /// Source mutation violates canonical model constraints.
    Model(ModelError),
    /// Sheet/name comparison requires unsupported identifier mapping.
    Identifier {
        /// Formula source being inspected when identifier comparison failed.
        source: FormulaSource,
        /// Underlying checked identifier diagnostic.
        error: crate::model::identifiers::IdentifierError,
    },
    /// Affected formula cannot be parsed or safely rewritten.
    Formula {
        /// Cell or defined-name identity of the rejected formula.
        source: FormulaSource,
        /// Underlying formula diagnostic.
        error: ParseError,
    },
}
impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Model(error) => error.fmt(f),
            Self::Identifier { source, error } => write!(f, "cannot rewrite {source:?}: {error}"),
            Self::Formula { source, error } => write!(f, "cannot rewrite {source:?}: {error}"),
        }
    }
}
impl std::error::Error for EditError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Model(e) => e,
            Self::Identifier { error, .. } => error,
            Self::Formula { error, .. } => error,
        })
    }
}
/// Options for renaming a worksheet and rewriting references to it.
///
/// Unlike a metadata-only rename, this rewrites supported references in cell
/// formulas and defined names. Untouched formulas keep their exact spelling;
/// rewritten formulas are printed canonically. Stored formula results are kept.
/// Any unsupported formula, including an unrelated one, leaves the workbook unchanged.
///
/// ```
/// use poirot::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "Data!B1")?;
/// let outcome = book.rename_sheet(0, "Inputs").apply()?;
/// assert_eq!(outcome.previous_name, "Data");
/// assert_eq!(book.stored_cell("Inputs", "A1".parse()?).ok_or("missing")?.formula.as_deref(),
///     Some("'Inputs'!B1"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
#[must_use = "Call apply to rename the sheet"]
pub struct RenameSheetBuilder<'a> {
    workbook: &'a mut Workbook,
    sheet: usize,
    name: String,
    parse: FormulaParsePolicy,
}
impl<'a> RenameSheetBuilder<'a> {
    /// Configure a rename of `sheet` (zero-based) to `name`.
    pub(crate) fn new(workbook: &'a mut Workbook, sheet: usize, name: String) -> Self {
        Self {
            workbook,
            sheet,
            name,
            parse: FormulaParsePolicy::default(),
        }
    }
    /// Replace the parser limits used to validate every affected formula.
    pub fn parse_policy(mut self, policy: FormulaParsePolicy) -> Self {
        self.parse = policy;
        self
    }
    /// Rename the sheet and rewrite its references as one atomic edit.
    pub fn apply(self) -> Result<EditOutcome, EditError> {
        let plan = plan_rename_sheet(self.workbook, self.sheet, self.name, self.parse)?;
        self.workbook
            .apply_rename_sheet_plan(&plan)
            .map_err(EditError::Model)
    }
}
/// Prepare checked source rewrites for a worksheet rename without mutating the workbook.
fn plan_rename_sheet(
    workbook: &Workbook,
    sheet: usize,
    name: String,
    policy: FormulaParsePolicy,
) -> Result<RenameSheetPlan, EditError> {
    // Check metadata first, while preserving the original model.
    RenameSheetPlan::new(workbook, sheet, name.clone(), vec![]).map_err(EditError::Model)?;
    let old = workbook
        .sheet(sheet)
        .ok_or_else(|| EditError::Model(ModelError("worksheet index out of bounds".into())))?
        .name();
    let mut replacements = Vec::new();
    for (sheet, worksheet) in workbook.sheets().iter().enumerate() {
        for (address, cell) in worksheet.cells() {
            if let Some(formula) = &cell.formula {
                plan_formula(
                    FormulaSource::Cell {
                        sheet,
                        address: *address,
                    },
                    formula,
                    old,
                    &name,
                    policy,
                    &mut replacements,
                )?;
            }
        }
    }
    for (index, defined) in workbook.named_ranges.iter().enumerate() {
        plan_formula(
            FormulaSource::DefinedName { index },
            &defined.formula,
            old,
            &name,
            policy,
            &mut replacements,
        )?;
    }
    RenameSheetPlan::new(workbook, sheet, name, replacements).map_err(EditError::Model)
}
fn plan_formula(
    source: FormulaSource,
    text: &str,
    old: &str,
    new: &str,
    policy: FormulaParsePolicy,
    replacements: &mut Vec<FormulaReplacement>,
) -> Result<(), EditError> {
    let parsed = parse_formula_with_policy(text, policy).map_err(|error| EditError::Formula {
        source: source.clone(),
        error,
    })?;
    let mut expression = parsed.into_expression();
    if rewrite(&mut expression, old, new).map_err(|error| EditError::Identifier {
        source: source.clone(),
        error,
    })? {
        let replacement =
            FormulaPrinter::print(&expression).map_err(|error| EditError::Formula {
                source: source.clone(),
                error,
            })?;
        replacements.push(FormulaReplacement {
            source,
            expected: text.into(),
            replacement,
        });
    }
    Ok(())
}
fn rewrite(
    expr: &mut Expr,
    old: &str,
    new: &str,
) -> Result<bool, crate::model::identifiers::IdentifierError> {
    Ok(match expr {
        Expr::Cell(c) => qualifier(&mut c.sheet, old, new)?,
        Expr::SheetSpan(span) => {
            let mut changed = false;
            for endpoint in [&mut span.first_sheet, &mut span.last_sheet] {
                let mut sheet = Some(endpoint.clone());
                changed |= qualifier(&mut sheet, old, new)?;
                if let Some(sheet) = sheet {
                    *endpoint = sheet;
                }
            }
            changed
        }
        Expr::QualifiedName { sheet, .. } => {
            let mut qualified = Some(sheet.clone());
            let changed = qualifier(&mut qualified, old, new)?;
            if let Some(qualified) = qualified {
                *sheet = qualified;
            }
            changed
        }
        Expr::Column { sheet, .. } | Expr::Row { sheet, .. } => qualifier(sheet, old, new)?,
        Expr::Unary { operand, .. } => rewrite(operand, old, new)?,
        Expr::Binary { left, right, .. } => {
            let a = rewrite(left, old, new)?;
            rewrite(right, old, new)? || a
        }
        Expr::Function { args, .. } => {
            let mut changed = false;
            for expr in args {
                changed |= rewrite(expr, old, new)?;
            }
            changed
        }
        Expr::Array(rows) => {
            let mut changed = false;
            for expr in rows.iter_mut().flatten() {
                changed |= rewrite(expr, old, new)?;
            }
            changed
        }
        _ => false,
    })
}
fn qualifier(
    sheet: &mut Option<String>,
    old: &str,
    new: &str,
) -> Result<bool, crate::model::identifiers::IdentifierError> {
    if let Some(current) = sheet
        && crate::model::identifiers::checked_matches(current, old)?
    {
        *sheet = Some(new.into());
        return Ok(true);
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CellValue, DateSystem};
    fn workbook() -> Workbook {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        w.add_sheet("Calc").unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value("A1".parse().unwrap(), 7.0)
            .unwrap();
        w.sheet_mut(1)
            .unwrap()
            .set_formula_with_cached_value(
                "A1".parse().unwrap(),
                "Data!A1+LEN(\"Data!A1\")",
                CellValue::Number(15.0),
            )
            .unwrap();
        w.define_name("Global", "Data!$A$1", None).unwrap();
        w.define_name("Local", "data!A1", Some(0)).unwrap();
        w
    }
    #[test]
    fn rename_rewrites_qualified_name_scope_and_preserves_ordinary_text() {
        let mut w = workbook();
        w.sheet_mut(1)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "Data!Local&\"Data!Local\"")
            .unwrap();
        let plan =
            plan_rename_sheet(&w, 0, "O'Brien".into(), FormulaParsePolicy::default()).unwrap();
        w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            w.sheet(1)
                .unwrap()
                .get("B1".parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some("'O''Brien'!Local&\"Data!Local\"")
        );
    }
    #[test]
    fn pure_plan_renames_references_names_and_preserves_literals_and_saved_values() {
        let mut w = workbook();
        let before = w.clone();
        let plan =
            plan_rename_sheet(&w, 0, "O'Brien Data".into(), FormulaParsePolicy::default()).unwrap();
        assert_eq!(w, before);
        assert_eq!(plan.replacements().len(), 3);
        let outcome = w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(outcome.changed_sources.len(), 3);
        assert_eq!(w.sheet(0).unwrap().name(), "O'Brien Data");
        let cell = w.sheet(1).unwrap().get("A1".parse().unwrap()).unwrap();
        assert_eq!(
            cell.formula.as_deref(),
            Some("'O''Brien Data'!A1+LEN(\"Data!A1\")")
        );
        assert_eq!(cell.value, CellValue::Number(15.0));
        assert_eq!(w.named_ranges[1].sheet_scope, Some(0));
        assert_eq!(w.named_ranges[0].formula, "'O''Brien Data'!$A$1");
    }
    #[test]
    fn cross_sheet_and_defined_names_evaluate_identically_after_rename() {
        let mut w = workbook();
        w.sheet_mut(1)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "Global+Data!A1")
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "Local+Global")
            .unwrap();
        let before_cross = match crate::eval::run_cell(
            &w,
            1,
            "B1".parse().unwrap(),
            Default::default(),
            Default::default(),
        ) {
            crate::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        let before_local = match crate::eval::run_cell(
            &w,
            0,
            "B1".parse().unwrap(),
            Default::default(),
            Default::default(),
        ) {
            crate::eval::EvaluationOutcome::Complete { value, .. } => value,
            outcome => panic!("evaluation aborted: {outcome:?}"),
        };
        assert_eq!(before_cross, CellValue::Number(14.0));
        assert_eq!(before_local, CellValue::Number(14.0));
        let plan =
            plan_rename_sheet(&w, 0, "O'Brien Data".into(), FormulaParsePolicy::default()).unwrap();
        w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            match crate::eval::run_cell(
                &w,
                1,
                "B1".parse().unwrap(),
                Default::default(),
                Default::default()
            ) {
                crate::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            before_cross
        );
        assert_eq!(
            match crate::eval::run_cell(
                &w,
                0,
                "B1".parse().unwrap(),
                Default::default(),
                Default::default()
            ) {
                crate::eval::EvaluationOutcome::Complete { value, .. } => value,
                outcome => panic!("evaluation aborted: {outcome:?}"),
            },
            before_local
        );
    }
    #[test]
    fn rewriting_visits_every_argument_and_array_element() {
        let mut expression =
            poirot_parsing::formula::parse_formula("SUM(Data!A1,1,Data!B1,Data!C1)").unwrap();
        assert!(rewrite(&mut expression, "Data", "Input").unwrap());
        assert_eq!(
            FormulaPrinter::print(&expression).unwrap(),
            "SUM('Input'!A1,1,'Input'!B1,'Input'!C1)"
        );
        // The parser currently restricts array literals to constants. Exercise
        // traversal directly so future array expression support cannot skip cells.
        let reference = poirot_parsing::formula::parse_formula("Data!A1").unwrap();
        let mut array = Expr::Array(vec![
            vec![reference.clone(), Expr::Number(1.0)],
            vec![reference.clone(), reference],
        ]);
        assert!(rewrite(&mut array, "Data", "Input").unwrap());
        let expected = poirot_parsing::formula::parse_formula("Input!A1").unwrap();
        assert_eq!(
            array,
            Expr::Array(vec![
                vec![expected.clone(), Expr::Number(1.0)],
                vec![expected.clone(), expected]
            ])
        );
    }
    #[test]
    fn literal_nan_identity_does_not_reject_an_unchanged_plan() {
        let mut w = workbook();
        let address = "C1".parse().unwrap();
        let nan = f64::from_bits(0x7ff8_0000_0000_0042);
        w.sheet_mut(0).unwrap().set_value(address, nan).unwrap();
        let plan = plan_rename_sheet(&w, 0, "Input".into(), FormulaParsePolicy::default()).unwrap();
        w.apply_rename_sheet_plan(&plan).unwrap();
        let plan = plan_rename_sheet(&w, 0, "Other".into(), FormulaParsePolicy::default()).unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_value(address, f64::from_bits(nan.to_bits() + 1))
            .unwrap();
        assert!(w.apply_rename_sheet_plan(&plan).is_err());
        assert_eq!(w.sheet(0).unwrap().name(), "Input");
        match w.sheet(0).unwrap().get(address).unwrap().value {
            CellValue::Number(value) => assert_eq!(value.to_bits(), nan.to_bits() + 1),
            _ => panic!("numeric literal changed type"),
        }
    }
    #[test]
    fn stale_and_unsupported_plans_fail_atomically() {
        let mut w = workbook();
        let plan = plan_rename_sheet(&w, 0, "Input".into(), FormulaParsePolicy::default()).unwrap();
        w.date_system = DateSystem::Excel1904;
        let before = w.clone();
        assert!(w.apply_rename_sheet_plan(&plan).is_err());
        assert_eq!(w, before);
        w.sheet_mut(1)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "[external.xlsx]Data!A1")
            .unwrap();
        let before = w.clone();
        assert!(plan_rename_sheet(&w, 0, "Input".into(), FormulaParsePolicy::default()).is_err());
        assert_eq!(w, before);
        assert!(plan_rename_sheet(&w, 0, "Calc".into(), FormulaParsePolicy::default()).is_err());
        assert_eq!(w, before);
    }
    #[test]
    fn cache_writeback_is_separate_from_source_and_untouched_spelling_survives() {
        let mut w = workbook();
        w.sheet_mut(1)
            .unwrap()
            .set_formula("B1".parse().unwrap(), "sum( 1 , 2 )")
            .unwrap();
        let plan = plan_rename_sheet(&w, 0, "Input".into(), FormulaParsePolicy::default()).unwrap();
        w.sheet_mut(1)
            .unwrap()
            .get_mut("A1".parse().unwrap())
            .unwrap()
            .value = CellValue::Number(999.0);
        w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            w.sheet(1)
                .unwrap()
                .get("B1".parse().unwrap())
                .unwrap()
                .formula
                .as_deref(),
            Some("sum( 1 , 2 )")
        );
        assert_eq!(
            w.sheet(1)
                .unwrap()
                .get("A1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(999.0)
        );
    }
}

#[cfg(test)]
mod identifier_edit_tests {
    use super::*;
    #[test]
    fn rename_uses_same_mapping_and_fails_atomically_on_unsupported_mapping() {
        let mut workbook = Workbook::new();
        workbook
            .add_sheet("Ä")
            .unwrap()
            .set_formula("A1".parse().unwrap(), "'ä'!B1")
            .unwrap();
        let plan =
            plan_rename_sheet(&workbook, 0, "Input".into(), FormulaParsePolicy::default()).unwrap();
        // The printer canonically quotes every sheet qualifier, even ASCII names.
        assert_eq!(plan.replacements()[0].replacement, "'Input'!B1");
        assert_eq!(
            poirot_parsing::formula::parse_formula(&plan.replacements()[0].replacement).unwrap(),
            poirot_parsing::formula::parse_formula("Input!B1").unwrap()
        );
        let mut unresolved = Workbook::new();
        unresolved
            .import_sheet("Σ")
            .unwrap()
            .set_formula("A1".parse().unwrap(), "'σ'!B1")
            .unwrap();
        let before = unresolved.clone();
        assert!(matches!(
            plan_rename_sheet(
                &unresolved,
                0,
                "Input".into(),
                FormulaParsePolicy::default()
            ),
            Err(EditError::Identifier { .. })
        ));
        assert_eq!(unresolved, before);
    }
}
