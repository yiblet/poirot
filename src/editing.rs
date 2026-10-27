//! Structural edits and source-checked formula reference translations.
//!
//! ```
//! use poirot::Workbook;
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_formula("A1", "Data!B1")?;
//! book.rename_sheet(0, "Inputs").apply()?;
//! assert!(book.sheet_by_name("Inputs").is_some());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::model::{CellValue, FormulaReplacement, ModelError, RenameSheetPlan, Workbook};
use poirot_parsing::formula::{
    Expr, FormulaParsePolicy, FormulaPrinter, TranslationPlan, parse_formula_with_policy,
};

pub use crate::edit_error::EditError;
pub use crate::model::{EditOutcome, FormulaSource, WorkbookChange};
pub use poirot_parsing::formula::{FormulaLocation, TranslationPolicy};

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
/// book.add_sheet("Data")?.set_formula("A1", "Data!B1")?;
/// let outcome = book.rename_sheet(0, "Inputs").apply()?;
/// assert_eq!(outcome.previous_name, "Data");
/// assert_eq!(book.stored_cell("Inputs", "A1")?.ok_or("missing")?.formula.as_deref(),
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
        .ok_or(EditError::Model(ModelError::InvalidWorksheetIndex {
            index: sheet,
        }))?
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
        target: source.clone(),
        error,
    })?;
    let mut expression = parsed.into_expression();
    if rewrite(&mut expression, old, new).map_err(|error| EditError::Identifier {
        target: source.clone(),
        error,
    })? {
        let replacement =
            FormulaPrinter::print(&expression).map_err(|error| EditError::Formula {
                target: source.clone(),
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
        w.sheet_mut(0).unwrap().set_value("A1", 7.0).unwrap();
        w.sheet_mut(1)
            .unwrap()
            .set_formula_with_cached_value(
                "A1",
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
            .set_formula("B1", "Data!Local&\"Data!Local\"")
            .unwrap();
        let plan =
            plan_rename_sheet(&w, 0, "O'Brien".into(), FormulaParsePolicy::default()).unwrap();
        w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            w.sheet(1)
                .unwrap()
                .get("B1")
                .unwrap()
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
        let cell = w.sheet(1).unwrap().get("A1").unwrap();
        assert_eq!(
            cell.unwrap().formula.as_deref(),
            Some("'O''Brien Data'!A1+LEN(\"Data!A1\")")
        );
        assert_eq!(cell.unwrap().value, CellValue::Number(15.0));
        assert_eq!(w.named_ranges[1].sheet_scope, Some(0));
        assert_eq!(w.named_ranges[0].formula, "'O''Brien Data'!$A$1");
    }
    #[test]
    fn cross_sheet_and_defined_names_evaluate_identically_after_rename() {
        let mut w = workbook();
        w.sheet_mut(1)
            .unwrap()
            .set_formula("B1", "Global+Data!A1")
            .unwrap();
        w.sheet_mut(0)
            .unwrap()
            .set_formula("B1", "Local+Global")
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
        let address = "C1".parse::<crate::model::CellAddress>().unwrap();
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
        match w.sheet(0).unwrap().get(address).unwrap().unwrap().value {
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
            .set_formula("B1", "[external.xlsx]Data!A1")
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
            .set_formula("B1", "sum( 1 , 2 )")
            .unwrap();
        let plan = plan_rename_sheet(&w, 0, "Input".into(), FormulaParsePolicy::default()).unwrap();
        w.sheet_mut(1)
            .unwrap()
            .get_mut("A1")
            .unwrap()
            .unwrap()
            .value = CellValue::Number(999.0);
        w.apply_rename_sheet_plan(&plan).unwrap();
        assert_eq!(
            w.sheet(1)
                .unwrap()
                .get("B1")
                .unwrap()
                .unwrap()
                .formula
                .as_deref(),
            Some("sum( 1 , 2 )")
        );
        assert_eq!(
            w.sheet(1).unwrap().get("A1").unwrap().unwrap().value,
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
            .set_formula("A1", "'ä'!B1")
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
            .set_formula("A1", "'σ'!B1")
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

/// A formula source and the copy displacement used to translate its relative references.
/// `to` defines the displacement; it does not move the formula to a new cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationRequest {
    /// Formula source identity to replace.
    pub source: FormulaSource,
    /// Original cell context for relative references.
    pub from: FormulaLocation,
    /// Destination cell context for relative references.
    pub to: FormulaLocation,
}
/// Options for translating references within existing formulas in place.
///
/// Each request translates relative references as if its formula were copied
/// by the request's displacement; no cells move. All requested formulas are
/// translated before any change is written, so one failure leaves the workbook
/// unchanged. Changed cell formulas lose their saved result and shared-formula
/// provenance.
///
/// ```
/// use poirot::{Workbook, editing::{FormulaLocation, FormulaSource, TranslationRequest}};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1", "A1*2")?;
/// let changed = book
///     .translate_formulas()
///     .request(TranslationRequest {
///         source: FormulaSource::Cell { sheet: 0, address: "B1".parse()? },
///         from: FormulaLocation { row: 0, column: 1 },
///         to: FormulaLocation { row: 1, column: 1 },
///     })
///     .apply()?;
/// assert_eq!(changed.len(), 1);
/// assert_eq!(book.stored_cell("Data", "B1")?.ok_or("missing")?.formula.as_deref(),
///     Some("A2*2"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
#[must_use = "Call apply to translate the formulas"]
pub struct TranslateFormulasBuilder<'a> {
    workbook: &'a mut Workbook,
    requests: Vec<TranslationRequest>,
    policy: TranslationPolicy,
}
impl<'a> TranslateFormulasBuilder<'a> {
    /// Configure an empty batch of translations for `workbook`.
    pub(crate) fn new(workbook: &'a mut Workbook) -> Self {
        Self {
            workbook,
            requests: Vec::new(),
            policy: TranslationPolicy::default(),
        }
    }
    /// Add one formula to translate. Each formula source may appear once.
    pub fn request(mut self, request: TranslationRequest) -> Self {
        self.requests.push(request);
        self
    }
    /// Replace the translation boundary and parsing policy.
    pub fn policy(mut self, policy: TranslationPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Translate every requested formula as one atomic edit.
    ///
    /// Returns the sources whose formula text changed.
    pub fn apply(self) -> Result<Vec<FormulaSource>, EditError> {
        FormulaTranslationEditPlan::new(self.workbook, &self.requests, self.policy)?
            .apply(self.workbook)
    }
}
/// A source-checked batch of in-place formula translations.
/// Application validates the captured sheet/name topology and formula sources before
/// changing any formula, and clears changed cell results.
#[derive(Debug, Clone, PartialEq)]
struct FormulaTranslationEditPlan {
    sheets: Vec<String>,
    names: Vec<(String, String, Option<usize>)>,
    edits: Vec<(FormulaSource, TranslationPlan)>,
}
impl FormulaTranslationEditPlan {
    /// Translate references in the selected formulas in place. This does not copy
    /// cells or move workbook structure; from/to specify the copy displacement.
    fn new(
        workbook: &Workbook,
        requests: &[TranslationRequest],
        policy: TranslationPolicy,
    ) -> Result<Self, EditError> {
        let mut seen = std::collections::BTreeSet::new();
        let mut edits = Vec::new();
        for request in requests {
            if !seen.insert(request.source.clone()) {
                return Err(EditError::DuplicateTranslationSource {
                    target: request.source.clone(),
                });
            }
            if let FormulaSource::Cell { address, .. } = &request.source
                && (address.row != request.from.row || address.col != request.from.column)
            {
                return Err(EditError::TranslationLocationMismatch {
                    target: request.source.clone(),
                });
            }
            let text = formula(workbook, &request.source).ok_or_else(|| {
                EditError::MissingTranslationFormula {
                    target: request.source.clone(),
                }
            })?;
            let plan =
                TranslationPlan::new(text, request.from, request.to, policy).map_err(|error| {
                    EditError::Formula {
                        target: request.source.clone(),
                        error,
                    }
                })?;
            edits.push((request.source.clone(), plan));
        }
        Ok(Self {
            sheets: sheet_names(workbook),
            names: defined_names(workbook),
            edits,
        })
    }
    /// All preconditions are checked before writes. Saved results and styles do
    /// not participate in source identity; changes to names/sheets invalidate it.
    fn apply(&self, workbook: &mut Workbook) -> Result<Vec<FormulaSource>, EditError> {
        if self.sheets != sheet_names(workbook) || self.names != defined_names(workbook) {
            return Err(EditError::StaleTranslationWorkbook);
        }
        for (source, plan) in &self.edits {
            let text =
                formula(workbook, source).ok_or_else(|| EditError::StaleTranslationFormula {
                    target: source.clone(),
                })?;
            plan.verify_source(text)
                .map_err(|error| EditError::Formula {
                    target: source.clone(),
                    error,
                })?;
        }
        let mut changed = Vec::new();
        for (source, plan) in &self.edits {
            if !plan.changed() {
                continue;
            }
            match source {
                FormulaSource::Cell { sheet, address } => {
                    let cell = workbook
                        .sheet_mut(*sheet)
                        .and_then(|sheet| sheet.get_mut_validated(*address))
                        .ok_or_else(|| EditError::StaleTranslationFormula {
                            target: source.clone(),
                        })?;
                    cell.formula = Some(plan.replacement().into());
                    cell.shared_formula = None;
                    cell.value = CellValue::Blank;
                }
                FormulaSource::DefinedName { index } => {
                    workbook
                        .named_ranges
                        .get_mut(*index)
                        .ok_or(EditError::MissingTranslationName { index: *index })?
                        .formula = plan.replacement().into()
                }
            }
            changed.push(source.clone());
        }
        Ok(changed)
    }
}

fn formula<'a>(workbook: &'a Workbook, source: &FormulaSource) -> Option<&'a str> {
    match source {
        FormulaSource::Cell { sheet, address } => workbook
            .sheet(*sheet)?
            .get_validated(*address)?
            .formula
            .as_deref(),
        FormulaSource::DefinedName { index } => Some(&workbook.named_ranges.get(*index)?.formula),
    }
}
fn sheet_names(workbook: &Workbook) -> Vec<String> {
    workbook.sheets().iter().map(|s| s.name().into()).collect()
}
fn defined_names(workbook: &Workbook) -> Vec<(String, String, Option<usize>)> {
    workbook
        .named_ranges
        .iter()
        .map(|n| (n.name.clone(), n.formula.clone(), n.sheet_scope))
        .collect()
}

#[cfg(test)]
mod translation_tests {
    use super::*;
    fn setup() -> (Workbook, Vec<TranslationRequest>) {
        let mut w = Workbook::new();
        w.add_sheet("Data").unwrap();
        let mut requests = Vec::new();
        for (address, text) in [("A1", "A1+$B$2"), ("A2", "A2+LEN(\"A2\")")] {
            let address: crate::model::CellAddress = address.parse().unwrap();
            w.sheet_mut(0)
                .unwrap()
                .set_formula_with_cached_value(address, text, CellValue::Number(17.0))
                .unwrap();
            requests.push(TranslationRequest {
                source: FormulaSource::Cell { sheet: 0, address },
                from: FormulaLocation {
                    row: address.row,
                    column: address.col,
                },
                to: FormulaLocation {
                    row: address.row + 1,
                    column: address.col + 1,
                },
            });
        }
        (w, requests)
    }
    #[test]
    fn pure_plan_atomic_stale_and_clear_changed_caches() {
        let (mut w, requests) = setup();
        let before = w.clone();
        let plan =
            FormulaTranslationEditPlan::new(&w, &requests, TranslationPolicy::default()).unwrap();
        assert_eq!(w, before);
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A2")
            .unwrap()
            .unwrap()
            .formula = Some("99".into());
        let stale = w.clone();
        assert!(plan.apply(&mut w).is_err());
        assert_eq!(w, stale);
        let (mut w, requests) = setup();
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A1")
            .unwrap()
            .unwrap()
            .shared_formula = Some(crate::model::SharedFormula {
            group: crate::model::SharedFormulaId(1),
            origin: "A1".parse().unwrap(),
        });
        let plan =
            FormulaTranslationEditPlan::new(&w, &requests, TranslationPolicy::default()).unwrap();
        assert_eq!(plan.apply(&mut w).unwrap().len(), 2);
        let cell = w.sheet(0).unwrap().get("A1").unwrap();
        assert_eq!(cell.unwrap().formula.as_deref(), Some("B2+$B$2"));
        assert_eq!(cell.unwrap().value, CellValue::Blank);
        assert_eq!(cell.unwrap().shared_formula, None);
    }
    #[test]
    fn unrepresentable_batch_and_sheet_identity_refuse() {
        let (mut w, requests) = setup();
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A2")
            .unwrap()
            .unwrap()
            .formula = Some("XFD2".into());
        let before = w.clone();
        assert!(
            FormulaTranslationEditPlan::new(&w, &requests, TranslationPolicy::default(),).is_err()
        );
        assert_eq!(w, before);
        let (mut w, requests) = setup();
        let plan =
            FormulaTranslationEditPlan::new(&w, &requests, TranslationPolicy::default()).unwrap();
        w.rename_sheet(0, "Other").apply().unwrap();
        let before = w.clone();
        assert!(plan.apply(&mut w).is_err());
        assert_eq!(w, before);
    }
}
