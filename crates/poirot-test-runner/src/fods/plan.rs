//! Validate source inputs and baseline bindings before evaluating any formula.
use super::workbook_preparation_error::WorkbookPreparationError;
use crate::fods::{
    FodsError,
    corpus::{BaselineCase, BaselineStatus, Corpus, FixtureEntry, load_fixture},
    report::{CaseFailure, FailureKind, NameDiagnostic, NameDisposition},
    xml::model::{FodsWorkbook, FormulaCase, InputValue, Omission},
};
use poirot::model::{CellAddress, CellValue, ModelError, Workbook};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    num::NonZeroUsize,
    path::PathBuf,
};

/// Corpus selection and bounded parallelism captured before fixture preparation.
#[derive(Clone, Debug)]
pub struct RunOptions {
    /// Corpus root containing metadata and native fixtures.
    pub corpus: PathBuf,
    /// Optional fixture-name substring filter.
    pub filter: Option<String>,
    /// Requested parallel worker count.
    pub jobs: NonZeroUsize,
}

/// Extracted check joined with baseline and preparation-time diagnostics.
#[derive(Debug)]
pub struct PlannedCase {
    /// Native source check to evaluate.
    pub source: FormulaCase,
    /// Recorded expectation used to classify run changes.
    pub baseline: BaselineStatus,
    /// Preparation failure retained without inventing a formula result.
    pub preparation_failure: Option<CaseFailure>,
    /// Unsupported function spellings retained for diagnostics.
    pub missing_functions: Vec<String>,
}

/// Prepared source workbook and checks shared by one fixture's execution.
#[derive(Debug)]
pub struct FixturePlan {
    /// Checked manifest identity for this fixture.
    pub entry: FixtureEntry,
    /// Canonical source or explicit unsupported-context preparation result.
    pub workbook: WorkbookPreparation,
    /// Immutable compiled formula index for the prepared workbook.
    pub evaluation_plan: Option<poirot::eval::CompiledWorkbook>,
    /// Source-ordered checks with joined baseline expectations.
    pub cases: Vec<PlannedCase>,
    /// Native calculation settings retained for execution context.
    pub calculation_settings: Vec<BTreeMap<String, String>>,
    /// Inspectable source constructs not represented by the canonical workbook.
    pub omissions: Vec<Omission>,
    /// Retained rejected declarations and their explicit preparation dispositions.
    pub preparation_diagnostics: Vec<NameDiagnostic>,
    /// Native CSE cells whose stored values cannot stand in for matrix execution.
    pub matrix_cells: BTreeSet<(usize, CellAddress)>,
}

/// Prepare a canonical workbook or retain why its source context is unsupported.
#[derive(Debug)]
pub enum WorkbookPreparation {
    /// Canonical source and compiled state are ready for execution.
    Ready(Box<Workbook>),
    /// Native workbook context cannot be represented faithfully.
    Unsupported(CaseFailure),
}

/// Pinned fixture plans and bounded concurrency for a complete corpus run.
#[derive(Debug)]
pub struct RunPlan {
    /// Pinned revision defining the planned fixtures.
    pub source_commit: String,
    /// Selected fixture plans in deterministic source order.
    pub fixtures: Vec<FixturePlan>,
    /// Worker count admitted for parallel execution.
    pub jobs: NonZeroUsize,
}

fn value(input: &InputValue) -> CellValue {
    match input {
        InputValue::Number { value, .. } => CellValue::Number(*value),
        InputValue::Text { value } => CellValue::Text(value.clone()),
        InputValue::Boolean { value } => CellValue::Boolean(*value),
        InputValue::Blank => CellValue::Blank,
    }
}

fn workbook(
    data: &FodsWorkbook,
    diagnostics: &mut Vec<NameDiagnostic>,
) -> Result<Workbook, WorkbookPreparationError> {
    let mut book = Workbook::new();
    for sheet in &data.sheets {
        book.add_sheet(&sheet.name)
            .map_err(WorkbookPreparationError::Model)?;
    }
    for (index, sheet) in data.sheets.iter().enumerate() {
        let dest = book
            .sheet_mut(index)
            .ok_or(WorkbookPreparationError::MissingWorksheet { index })?;
        for cell in &sheet.cells {
            let address = CellAddress {
                row: cell.row,
                col: cell.col,
            };
            if let Some(formula) = &cell.formula {
                dest.set_formula(address, formula)
                    .map_err(|source| WorkbookPreparationError::Model(source.into()))?;
            } else {
                dest.set_value(address, value(&cell.value))
                    .map_err(|source| WorkbookPreparationError::Model(source.into()))?;
            }
        }
    }
    let mut blocked = None;
    for (index, name) in data.names.iter().enumerate() {
        if let Err(error) = book.define_name(&name.name, &name.formula, None) {
            let rejection = error.to_string();
            // Only lexical name rejection is eligible. Duplicate, formula and scope
            // errors retain their existing preparation failure without quarantine.
            if !matches!(error, ModelError::InvalidDefinedName { .. }) {
                blocked.get_or_insert(WorkbookPreparationError::Model(error));
                continue;
            }
            let proof = super::name_admission::prove_unused(data, &name.name);
            let (disposition, reason) = match proof {
                Ok(()) => (NameDisposition::QuarantinedUnused,
                    "bounded inspection of every original cell, matrix anchor and name definition found no potential use or runtime resolver".to_owned()),
                Err(reason) => {
                    let message = crate::report::error_message(&reason);
                    blocked.get_or_insert(WorkbookPreparationError::RejectedName { name: name.name.clone(), source: reason });
                    (NameDisposition::BlocksPreparation, message)
                }
            };
            diagnostics.push(NameDiagnostic {
                source: data.source.clone(),
                source_commit: data.source_commit.clone(),
                sha256: data.sha256.clone(),
                declaration: index,
                name: name.name.clone(),
                original_definition: name.original_definition.clone(),
                base_cell_address: name.base_cell_address.clone(),
                rejection,
                disposition,
                reason,
            });
        }
    }
    if let Some(reason) = blocked {
        return Err(reason);
    }
    Ok(book)
}

/// Validate fixture provenance and prepare its workbook, compilation, and baseline cases.
pub(crate) fn prepare_fixture(
    entry: FixtureEntry,
    data: FodsWorkbook,
    baseline: &HashMap<String, BaselineCase>,
) -> Result<FixturePlan, FodsError> {
    if data
        .sheets
        .iter()
        .flat_map(|sheet| &sheet.cells)
        .any(|cell| (cell.formula.is_some() || cell.matrix.is_some()) && !cell.value.is_blank())
    {
        return Err(FodsError::ComputedSourceCache {
            fixture: entry.fixture.clone(),
        });
    }
    let mut preparation_diagnostics = Vec::new();
    let book = match workbook(&data, &mut preparation_diagnostics) {
        Ok(book) => WorkbookPreparation::Ready(Box::new(book)),
        Err(message) => WorkbookPreparation::Unsupported(CaseFailure {
            kind: FailureKind::WorkbookContext,
            gap_type: crate::fods::gaps::GapKind::UnsupportedWorkbookContext,
            missing_functions: Vec::new(),
            message: crate::report::error_message(&message),
        }),
    };
    let evaluation_plan = match &book {
        WorkbookPreparation::Ready(book) => Some(
            poirot::eval::FormulaCompiler::default()
                .compile_workbook(book, &poirot::eval::PreparationPolicy::default()),
        ),
        WorkbookPreparation::Unsupported(_) => None,
    };
    let mut cases = Vec::with_capacity(data.cases.len());
    for (index, source) in data.cases.into_iter().enumerate() {
        let bound = baseline
            .get(&source.id)
            .ok_or_else(|| FodsError::MissingBaselineCase {
                case: source.id.clone(),
            })?;
        if bound.fixture != entry.fixture
            || bound.index != index
            || bound
                .sheet
                .as_deref()
                .is_some_and(|sheet| sheet != source.sheet_name)
            || bound
                .cell
                .as_deref()
                .is_some_and(|cell| cell != source.cell)
        {
            return Err(FodsError::BaselineIdentityMismatch {
                case: source.id.clone(),
            });
        }
        if data
            .sheets
            .get(source.sheet)
            .is_none_or(|sheet| sheet.name != source.sheet_name)
            || source.row >= poirot::model::MAX_ROWS
            || source.col >= poirot::model::MAX_COLUMNS
            || source.correct_row >= poirot::model::MAX_ROWS
            || source.correct_col >= poirot::model::MAX_COLUMNS
        {
            return Err(FodsError::InvalidCaseCoordinates {
                case: source.id.clone(),
            });
        }
        let parsed = poirot_parsing::formula::parse_formula(&source.formula);
        let missing_functions = parsed
            .as_ref()
            .map(crate::fods::gaps::missing_root_function)
            .unwrap_or_default();
        let preparation_failure = parsed.err().map(|error| {
            let (kind, gap_type) = match error.kind() {
                poirot_parsing::formula::ParseErrorKind::Syntax => (
                    FailureKind::SourceSyntax,
                    crate::fods::gaps::GapKind::UnsupportedFormulaSyntax,
                ),
                poirot_parsing::formula::ParseErrorKind::ResourceLimit => (
                    FailureKind::EvaluationAbort,
                    crate::fods::gaps::GapKind::ResourceLimit,
                ),
                poirot_parsing::formula::ParseErrorKind::InvalidPolicy => (
                    FailureKind::EvaluationAbort,
                    crate::fods::gaps::GapKind::UnsupportedCapability,
                ),
            };
            CaseFailure {
                kind,
                gap_type,
                missing_functions: Vec::new(),
                message: format!("source formula preparation failed: {error:?}"),
            }
        });
        cases.push(PlannedCase {
            source,
            baseline: bound.classification,
            preparation_failure,
            missing_functions,
        });
    }
    let matrix_cells = data
        .sheets
        .iter()
        .enumerate()
        .flat_map(|(sheet, source)| {
            source
                .cells
                .iter()
                .filter(|cell| cell.matrix.is_some())
                .map(move |cell| {
                    (
                        sheet,
                        CellAddress {
                            row: cell.row,
                            col: cell.col,
                        },
                    )
                })
        })
        .collect();
    Ok(FixturePlan {
        entry,
        workbook: book,
        evaluation_plan,
        cases,
        calculation_settings: data.calculation_settings,
        omissions: data.omissions,
        matrix_cells,
        preparation_diagnostics,
    })
}

/// Load the corpus and prepare selected fixtures before evaluation begins.
pub fn build(options: &RunOptions) -> Result<RunPlan, FodsError> {
    let corpus = Corpus::open(&options.corpus)?;
    let selected: Vec<_> = corpus
        .manifest
        .fixtures
        .iter()
        .filter(|entry| {
            options
                .filter
                .as_ref()
                .is_none_or(|filter| entry.fixture.contains(filter))
        })
        .collect();
    if selected.is_empty() {
        return Err(FodsError::EmptySelection);
    }
    let mut fixtures = Vec::with_capacity(selected.len());
    for entry in selected {
        let source = load_fixture(&corpus.root, entry)?;
        fixtures.push(prepare_fixture(entry.clone(), source, &corpus.baseline)?);
    }
    Ok(RunPlan {
        source_commit: corpus.manifest.source_commit,
        fixtures,
        jobs: options.jobs,
    })
}
