//! Select and schedule suites while preserving source-specific execution contracts.
use crate::{
    Error,
    baseline::Baseline,
    invalid,
    model::*,
    report::{self, Format},
    selection::Selection,
    skips::Skips,
    sources::{self, TestSuite},
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    num::NonZeroUsize,
    path::PathBuf,
};

/// Inputs and execution policy for discovering and running the unified workbook test corpora.
/// The same selection drives discovery and execution; strict mode also fails retained known gaps and skips.
#[derive(Debug)]
pub struct Options {
    pub fods_corpus: PathBuf,
    pub json_corpus: PathBuf,
    pub baseline: PathBuf,
    pub selection: Selection,
    pub skips: Option<PathBuf>,
    pub jobs: NonZeroUsize,
    pub strict: bool,
}

impl Default for Options {
    fn default() -> Self {
        let root = crate::package_root();
        Self {
            fods_corpus: root.join("corpus/fods"),
            json_corpus: root.join("corpus/json"),
            baseline: root.join("baseline.json"),
            selection: Selection::default(),
            skips: None,
            jobs: crate::fods::cli::default_jobs(),
            strict: false,
        }
    }
}

/// Run provenance retained with saved results so reports and baseline candidates
/// can verify the source revision, baseline identity, and discovered inventory.
/// The completeness flag distinguishes a full default-corpus run from a partial selection.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Header {
    pub schema_version: u8,
    pub baseline_path: PathBuf,
    pub baseline_digest: String,
    pub fods_source_commit: String,
    pub selection: Selection,
    pub strict: bool,
    pub workers: usize,
    pub complete_inventory: bool,
    pub inventory: Vec<CaseMetadata>,
}

/// Aggregate execution outcomes and baseline changes used to decide whether a run succeeds.
/// Success requires selected cases, no removed tests, and no case failing the run's strictness policy.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub selected: usize,
    pub outcomes: BTreeMap<Outcome, usize>,
    pub changes: BTreeMap<Change, usize>,
    pub removed: usize,
    pub successful: bool,
}

/// Saved unified run containing provenance, per-case observations, and the aggregate decision.
/// Removed baseline IDs remain explicit so a partial or changed inventory cannot
/// silently erase test history when proposing a new baseline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EvaluationReport {
    pub header: Header,
    pub cases: Vec<CaseResult>,
    pub removed: Vec<String>,
    pub summary: Summary,
}

impl EvaluationReport {
    pub fn summarize(&mut self) {
        let mut summary = Summary {
            selected: self.cases.len(),
            removed: self.removed.len(),
            successful: !self.cases.is_empty() && self.removed.is_empty(),
            ..Summary::default()
        };
        for case in &self.cases {
            *summary
                .outcomes
                .entry(case.observation.outcome)
                .or_default() += 1;
            *summary.changes.entry(case.change).or_default() += 1;
            summary.successful &= !case.fails(self.header.strict);
        }
        self.summary = summary;
    }
}

pub(crate) fn discover(options: &Options) -> Result<Vec<Box<dyn TestSuite>>, Error> {
    let mut suites = Vec::new();
    if options.selection.source.is_empty() || options.selection.source.contains(&Source::Fods) {
        suites.extend(sources::fods(&options.fods_corpus)?);
    }
    if options.selection.source.is_empty() || options.selection.source.contains(&Source::Json) {
        suites.extend(sources::json(&options.json_corpus)?);
    }
    let mut seen = BTreeSet::new();
    for case in suites.iter().flat_map(|suite| suite.cases()) {
        if !seen.insert(case.id.clone()) {
            return Err(invalid(format!("duplicate test ID: {}", case.id)));
        }
    }
    Ok(suites)
}

pub fn list(options: &Options) -> Result<Vec<CaseMetadata>, Error> {
    let cases: Vec<_> = discover(options)?
        .into_iter()
        .flat_map(|suite| suite.cases().to_vec())
        .filter(|case| options.selection.matches(case))
        .collect();
    if cases.is_empty() {
        return Err(invalid(
            "no tests match the selection; use list without filters to see available groups",
        ));
    }
    Ok(cases)
}

pub fn run(
    options: &Options,
    output: &mut impl Write,
    format: Format,
    verbose: bool,
    quiet: bool,
) -> Result<EvaluationReport, Error> {
    let baseline = Baseline::load(&options.baseline)?;
    let suites = discover(options)?;
    let inventory: Vec<_> = suites
        .iter()
        .flat_map(|suite| suite.cases().iter().cloned())
        .collect();
    let skips = Skips::load(
        options.skips.as_deref(),
        &inventory,
        &options.selection.source,
    )?;
    let complete_inventory = options.selection.unrestricted()
        && options.fods_corpus == Options::default().fods_corpus
        && options.json_corpus == Options::default().json_corpus;
    let selected: BTreeSet<_> = inventory
        .iter()
        .filter(|case| options.selection.matches(case))
        .map(|case| case.id.clone())
        .collect();
    if selected.is_empty() {
        return Err(invalid(
            "no tests match the selection; use list to preview it",
        ));
    }
    let removed = if complete_inventory {
        let ids: BTreeSet<_> = inventory.iter().map(|case| &case.id).collect();
        baseline
            .cases
            .keys()
            .filter(|id| !ids.contains(id))
            .cloned()
            .collect()
    } else {
        Vec::new()
    };
    let header = Header {
        schema_version: 1,
        baseline_path: options.baseline.clone(),
        baseline_digest: digest(&std::fs::read(&options.baseline)?),
        fods_source_commit: crate::fods::xml::SOURCE_COMMIT.into(),
        selection: options.selection.clone(),
        strict: options.strict,
        workers: options.jobs.get(),
        complete_inventory,
        inventory,
    };
    let mut report = EvaluationReport {
        header,
        cases: Vec::new(),
        removed,
        summary: Summary::default(),
    };
    report::begin(output, format, &report.header, selected.len())?;
    for case in report
        .header
        .inventory
        .iter()
        .filter(|case| selected.contains(&case.id))
    {
        if let Some(reason) = skips.reason(&case.id) {
            let mut result = CaseResult {
                case: case.clone(),
                observation: Observation {
                    outcome: Outcome::Skipped,
                    category: Some("explicit_skip".into()),
                    fingerprint: Some(digest(reason.as_bytes())),
                },
                reason,
                details: serde_json::Value::Null,
                change: Change::Unchanged,
                baseline: None,
            };
            baseline.compare(&mut result);
            report::suite(
                output,
                format,
                std::slice::from_ref(&result),
                verbose,
                quiet,
            )?;
            report.cases.push(result);
        }
    }
    let work: Vec<_> = suites
        .iter()
        .filter_map(|suite| {
            let ids: BTreeSet<_> = suite
                .cases()
                .iter()
                .filter(|case| selected.contains(&case.id) && skips.reason(&case.id).is_none())
                .map(|case| case.id.clone())
                .collect();
            (!ids.is_empty()).then_some((suite.as_ref(), ids))
        })
        .collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.jobs.get())
        .stack_size(16 * 1024 * 1024)
        .thread_name(|i| format!("test-suite-{i}"))
        .build()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| -> Result<(), Error> {
        scope.spawn(move || {
            pool.install(|| {
                work.par_iter()
                    .for_each_with(sender, |sender, (suite, ids)| {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            suite.run(ids)
                        }))
                        .map_err(|_| "suite evaluator panicked".to_owned())
                        .and_then(|result| result.map_err(|error| error.to_string()));
                        let cases = suite
                            .cases()
                            .iter()
                            .filter(|case| ids.contains(&case.id))
                            .cloned()
                            .collect::<Vec<_>>();
                        let _ = sender.send((cases, result));
                    })
            })
        });
        for (cases, result) in receiver {
            let mut results = match result {
                Ok(results) => results,
                Err(reason) => cases
                    .into_iter()
                    .map(|case| CaseResult {
                        case,
                        observation: Observation {
                            outcome: Outcome::HarnessError,
                            category: Some("suite_error".into()),
                            fingerprint: Some(digest(reason.as_bytes())),
                        },
                        reason: reason.clone(),
                        details: serde_json::Value::Null,
                        change: Change::Unchanged,
                        baseline: None,
                    })
                    .collect(),
            };
            for result in &mut results {
                baseline.compare(result);
            }
            report::suite(output, format, &results, verbose, quiet)?;
            report.cases.extend(results);
        }
        Ok(())
    })?;
    report.cases.sort_by(|a, b| a.case.id.cmp(&b.case.id));
    report.summarize();
    report::finish(output, format, &report)?;
    Ok(report)
}
