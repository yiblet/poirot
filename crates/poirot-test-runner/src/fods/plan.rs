//! Validate source inputs and baseline bindings before evaluating any formula.
use crate::fods::{
    Error,
    corpus::{BaselineCase, BaselineStatus, Corpus, FixtureEntry, load_fixture},
    report::{CaseFailure, FailureKind},
    xml::model::{FodsWorkbook, FormulaCase, InputValue, Omission},
};
use poirot_model::{CellAddress, CellValue, Workbook};
use std::{
    collections::{BTreeMap, HashMap},
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
    pub syntax_failure: Option<CaseFailure>,
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
    pub evaluation_plan: Option<poirot_eval::CompiledWorkbook>,
    /// Source-ordered checks with joined baseline expectations.
    pub cases: Vec<PlannedCase>,
    /// Native calculation settings retained for execution context.
    pub calculation_settings: Vec<BTreeMap<String, String>>,
    /// Inspectable source constructs not represented by the canonical workbook.
    pub omissions: Vec<Omission>,
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

fn workbook(data: &FodsWorkbook) -> Result<Workbook, String> {
    let mut book = Workbook::new();
    for sheet in &data.sheets {
        book.add_sheet(&sheet.name).map_err(|e| e.to_string())?;
    }
    for (index, sheet) in data.sheets.iter().enumerate() {
        let dest = book.sheet_mut(index).ok_or("missing worksheet")?;
        for cell in &sheet.cells {
            let address = CellAddress {
                row: cell.row,
                col: cell.col,
            };
            if let Some(formula) = &cell.formula {
                dest.set_formula(address, formula, CellValue::Blank)
                    .map_err(|e| e.to_string())?;
            } else {
                dest.set_value(address, value(&cell.value))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    for name in &data.names {
        book.define_name(&name.name, &name.formula, None)
            .map_err(|e| e.to_string())?;
    }
    Ok(book)
}

/// Validate fixture provenance and prepare its workbook, compilation, and baseline cases.
pub(crate) fn prepare_fixture(
    entry: FixtureEntry,
    data: FodsWorkbook,
    baseline: &HashMap<String, BaselineCase>,
) -> Result<FixturePlan, Error> {
    if data
        .sheets
        .iter()
        .flat_map(|sheet| &sheet.cells)
        .any(|cell| (cell.formula.is_some() || cell.matrix.is_some()) && !cell.value.is_blank())
    {
        return Err(Error::Workbook {
            fixture: entry.fixture.clone(),
            message: "computed source caches must be blank before planning".into(),
        });
    }
    let book = match workbook(&data) {
        Ok(book) => WorkbookPreparation::Ready(Box::new(book)),
        Err(message) => WorkbookPreparation::Unsupported(CaseFailure {
            kind: FailureKind::WorkbookContext,
            gap_type: crate::fods::gaps::GapKind::UnsupportedWorkbookContext,
            missing_functions: Vec::new(),
            message,
        }),
    };
    let evaluation_plan = match &book {
        WorkbookPreparation::Ready(book) => Some(
            poirot_eval::FormulaCompiler::default()
                .compile_workbook(book, &poirot_eval::EvaluationPolicy::default()),
        ),
        WorkbookPreparation::Unsupported(_) => None,
    };
    let mut cases = Vec::with_capacity(data.cases.len());
    for (index, source) in data.cases.into_iter().enumerate() {
        let bound = baseline.get(&source.id).ok_or_else(|| {
            Error::InvalidPlan(format!("case {} missing from baseline", source.id))
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
            return Err(Error::InvalidPlan(format!(
                "baseline identity differs from native source case {}",
                source.id
            )));
        }
        if data
            .sheets
            .get(source.sheet)
            .is_none_or(|sheet| sheet.name != source.sheet_name)
            || source.row >= poirot_model::MAX_ROWS
            || source.col >= poirot_model::MAX_COLUMNS
            || source.correct_row >= poirot_model::MAX_ROWS
            || source.correct_col >= poirot_model::MAX_COLUMNS
        {
            return Err(Error::InvalidPlan(format!(
                "invalid sheet or coordinates in case {}",
                source.id
            )));
        }
        let parsed = poirot_parsing::formula::parse_formula(&source.formula);
        let missing_functions = parsed
            .as_ref()
            .map(crate::fods::gaps::missing_root_function)
            .unwrap_or_default();
        let syntax_failure = parsed.err().map(|error| CaseFailure {
            kind: FailureKind::SourceSyntax,
            gap_type: crate::fods::gaps::GapKind::UnsupportedFormulaSyntax,
            missing_functions: Vec::new(),
            message: format!("unsupported source formula syntax: {error:?}"),
        });
        cases.push(PlannedCase {
            source,
            baseline: bound.classification,
            syntax_failure,
            missing_functions,
        });
    }
    Ok(FixturePlan {
        entry,
        workbook: book,
        evaluation_plan,
        cases,
        calculation_settings: data.calculation_settings,
        omissions: data.omissions,
    })
}

/// Load the corpus and prepare selected fixtures before evaluation begins.
pub fn build(options: &RunOptions) -> Result<RunPlan, Error> {
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
        return Err(Error::InvalidPlan("no fixtures match the selection".into()));
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
