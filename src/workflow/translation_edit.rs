//! Atomic application of bounded syntax translations to existing formula sources.
use crate::model::{CellValue, FormulaSource, ModelError, Workbook};
use crate::workflow::edit::EditError;
use poirot_parsing::formula::{FormulaLocation, TranslationPlan, TranslationPolicy};

/// Treatment of saved formula results after an in-place reference translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TranslationCachePolicy {
    /// Changed cell formulas lose their imported result; runtime caches key by source.
    #[default]
    ClearChanged,
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
/// A source-checked batch of in-place formula translations.
/// Application validates the captured sheet/name topology and formula sources before
/// changing any formula, and clears changed cell results under the default policy.
#[derive(Debug, Clone, PartialEq)]
pub struct FormulaTranslationEditPlan {
    sheets: Vec<String>,
    names: Vec<(String, String, Option<usize>)>,
    edits: Vec<(FormulaSource, TranslationPlan)>,
    cache_policy: TranslationCachePolicy,
}
impl FormulaTranslationEditPlan {
    /// Translate references in the selected formulas in place. This does not copy
    /// cells or move workbook structure; from/to specify the copy displacement.
    pub fn new(
        workbook: &Workbook,
        requests: &[TranslationRequest],
        policy: TranslationPolicy,
        cache_policy: TranslationCachePolicy,
    ) -> Result<Self, EditError> {
        let mut seen = std::collections::BTreeSet::new();
        let mut edits = Vec::new();
        for request in requests {
            if !seen.insert(request.source.clone()) {
                return Err(model_error("duplicate translation source"));
            }
            if let FormulaSource::Cell { address, .. } = &request.source
                && (address.row != request.from.row || address.col != request.from.column)
            {
                return Err(model_error(
                    "translation source location does not match cell",
                ));
            }
            let text = formula(workbook, &request.source)
                .ok_or_else(|| model_error("missing translation formula source"))?;
            let plan =
                TranslationPlan::new(text, request.from, request.to, policy).map_err(|error| {
                    EditError::Formula {
                        source: request.source.clone(),
                        error,
                    }
                })?;
            edits.push((request.source.clone(), plan));
        }
        Ok(Self {
            sheets: sheet_names(workbook),
            names: defined_names(workbook),
            edits,
            cache_policy,
        })
    }
    /// Borrow checked source replacements to apply as one edit.
    pub fn edits(&self) -> &[(FormulaSource, TranslationPlan)] {
        &self.edits
    }
    /// Return the saved-result policy captured during planning.
    pub fn cache_policy(&self) -> TranslationCachePolicy {
        self.cache_policy
    }
    /// All preconditions are checked before writes. Saved results and styles do
    /// not participate in source identity; changes to names/sheets invalidate it.
    pub fn apply(&self, workbook: &mut Workbook) -> Result<Vec<FormulaSource>, EditError> {
        if self.sheets != sheet_names(workbook) || self.names != defined_names(workbook) {
            return Err(model_error("stale translation workbook identity"));
        }
        for (source, plan) in &self.edits {
            let text = formula(workbook, source)
                .ok_or_else(|| model_error("stale translation formula source"))?;
            plan.verify_source(text)
                .map_err(|error| EditError::Formula {
                    source: source.clone(),
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
                        .and_then(|sheet| sheet.get_mut(*address))
                        .ok_or_else(|| model_error("stale translation formula source"))?;
                    cell.formula = Some(plan.replacement().into());
                    cell.shared_formula = None;
                    match self.cache_policy {
                        TranslationCachePolicy::ClearChanged => cell.value = CellValue::Blank,
                    }
                }
                FormulaSource::DefinedName { index } => {
                    workbook
                        .named_ranges
                        .get_mut(*index)
                        .ok_or_else(|| model_error("stale translation defined name"))?
                        .formula = plan.replacement().into()
                }
            }
            changed.push(source.clone());
        }
        Ok(changed)
    }
}
fn model_error(text: &str) -> EditError {
    EditError::Model(ModelError(text.into()))
}
fn formula<'a>(workbook: &'a Workbook, source: &FormulaSource) -> Option<&'a str> {
    match source {
        FormulaSource::Cell { sheet, address } => {
            workbook.sheet(*sheet)?.get(*address)?.formula.as_deref()
        }
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
mod tests {
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
        let plan = FormulaTranslationEditPlan::new(
            &w,
            &requests,
            TranslationPolicy::default(),
            TranslationCachePolicy::default(),
        )
        .unwrap();
        assert_eq!(w, before);
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A2".parse().unwrap())
            .unwrap()
            .formula = Some("99".into());
        let stale = w.clone();
        assert!(plan.apply(&mut w).is_err());
        assert_eq!(w, stale);
        let (mut w, requests) = setup();
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A1".parse().unwrap())
            .unwrap()
            .shared_formula = Some(crate::model::SharedFormula {
            group: crate::model::SharedFormulaId(1),
            origin: "A1".parse().unwrap(),
        });
        let plan = FormulaTranslationEditPlan::new(
            &w,
            &requests,
            TranslationPolicy::default(),
            TranslationCachePolicy::default(),
        )
        .unwrap();
        assert_eq!(plan.apply(&mut w).unwrap().len(), 2);
        let cell = w.sheet(0).unwrap().get("A1".parse().unwrap()).unwrap();
        assert_eq!(cell.formula.as_deref(), Some("B2+$B$2"));
        assert_eq!(cell.value, CellValue::Blank);
        assert_eq!(cell.shared_formula, None);
    }
    #[test]
    fn unrepresentable_batch_and_sheet_identity_refuse() {
        let (mut w, requests) = setup();
        w.sheet_mut(0)
            .unwrap()
            .get_mut("A2".parse().unwrap())
            .unwrap()
            .formula = Some("XFD2".into());
        let before = w.clone();
        assert!(
            FormulaTranslationEditPlan::new(
                &w,
                &requests,
                TranslationPolicy::default(),
                TranslationCachePolicy::default()
            )
            .is_err()
        );
        assert_eq!(w, before);
        let (mut w, requests) = setup();
        let plan = FormulaTranslationEditPlan::new(
            &w,
            &requests,
            TranslationPolicy::default(),
            TranslationCachePolicy::default(),
        )
        .unwrap();
        w.rename_sheet(0, "Other").unwrap();
        let before = w.clone();
        assert!(plan.apply(&mut w).is_err());
        assert_eq!(w, before);
    }
}
