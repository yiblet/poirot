//! Execute immutable fixture plans on an isolated Rayon pool.
use crate::fods::{
    Error,
    corpus::BaselineStatus,
    gaps::{self, GapKind},
    plan::{FixturePlan, PlannedCase, RunPlan, WorkbookPreparation},
    report::{CaseFailure, CaseResult, CaseStatus, FailureKind, RunReport, RunSummary},
};
use poirot_model::{CellAddress, CellValue, Workbook};
use rayon::prelude::*;

fn failure(kind: FailureKind, message: impl Into<String>) -> CaseFailure {
    CaseFailure {
        kind,
        gap_type: match kind {
            FailureKind::WorkbookContext => GapKind::UnsupportedWorkbookContext,
            FailureKind::SourceSyntax => GapKind::UnsupportedFormulaSyntax,
            FailureKind::UnsupportedName => GapKind::NameResolution,
            FailureKind::EvaluationAbort => GapKind::UnsupportedCapability,
            FailureKind::MatrixContext => GapKind::UnsupportedArrayContext,
            FailureKind::PredicateMismatch => GapKind::PredicateMismatch,
            FailureKind::EvaluatorPanic => GapKind::HarnessError,
        },
        missing_functions: Vec::new(),
        message: message.into(),
    }
}

fn aborted(abort: poirot_eval::EvaluationFailure) -> CaseFailure {
    let mut reason = failure(
        FailureKind::EvaluationAbort,
        format!("evaluation aborted: {abort:?}"),
    );
    reason.gap_type = match abort {
        poirot_eval::EvaluationFailure::Budget(_)
        | poirot_eval::EvaluationFailure::ResourceLimit { .. }
        | poirot_eval::EvaluationFailure::MemoRetentionLimit { .. } => GapKind::ResourceLimit,
        poirot_eval::EvaluationFailure::UnsupportedCapability { .. } => {
            GapKind::UnsupportedCapability
        }
        poirot_eval::EvaluationFailure::KnownSourceException { .. } => GapKind::SourceException,
        _ => GapKind::HarnessError,
    };
    reason
}

fn evaluate(
    book: &Workbook,
    sheet: usize,
    at: CellAddress,
    formula: &str,
) -> Result<CellValue, CaseFailure> {
    match poirot_eval::run_formula(
        book,
        sheet,
        at,
        formula,
        poirot_eval::EvaluationInputs::default(),
        poirot_eval::EvaluationBudgets::default(),
    ) {
        poirot_eval::EvaluationOutcome::Complete { value, .. } => Ok(value),
        poirot_eval::EvaluationOutcome::Aborted { failure: abort, .. } => Err(aborted(abort)),
    }
}

/// Typed facts from an executed failing predicate, kept separate from native report compatibility.
pub(crate) struct CheckObservation {
    pub actual: CellValue,
    pub expected: Option<CellValue>,
    pub predicate: CellValue,
}

fn check(
    book: &Workbook,
    prepared: &poirot_eval::CompiledWorkbook,
    entry: &crate::fods::corpus::FixtureEntry,
    plan: &PlannedCase,
    observed: &mut Option<CheckObservation>,
) -> Result<(), CaseFailure> {
    if let Some(reason) = &plan.syntax_failure {
        return Err(reason.clone());
    }
    let case = &plan.source;
    let borrow_failed = std::cell::Cell::new(false);
    let trace = std::cell::RefCell::new(crate::fods::execution_trace::Trace::default());
    let mut reads = crate::fods::execution_trace::Reads {
        borrow_failed: &borrow_failed,
        trace: &trace,
        plan: prepared,
        entry,
        book,
    };
    let mut cells = crate::fods::execution_trace::Cells {
        trace: &trace,
        plan: prepared,
    };
    let view =
        poirot_eval::WorkbookView::new(book, prepared, prepared.policy()).map_err(aborted)?;
    let configuration = poirot_eval::RunConfiguration::default();
    let mut run = poirot_eval::EvaluationRun::new_with_configuration(
        view,
        &configuration,
        poirot_eval::EvaluationBudgets::default(),
    )
    .map_err(aborted)?
    .with_dependency_sink(&mut reads)
    .with_cell_observer(&mut cells);
    let target = |row, col| poirot_eval::EvaluationTarget {
        sheet: case.sheet,
        address: CellAddress { row, col },
    };
    let actual = run
        .evaluate_cell(target(case.row, case.col))
        .map_err(aborted)?;
    if borrow_failed.get() {
        return Err(aborted(poirot_eval::EvaluationFailure::StalePlan));
    }
    if let Some(reason) = trace
        .try_borrow()
        .map_err(|_| aborted(poirot_eval::EvaluationFailure::StalePlan))?
        .failure("actual")
    {
        return Err(reason);
    }
    let actual_unresolved_name = trace
        .try_borrow()
        .map_err(|_| aborted(poirot_eval::EvaluationFailure::StalePlan))?
        .unresolved_name()
        .map(str::to_owned);
    *trace
        .try_borrow_mut()
        .map_err(|_| aborted(poirot_eval::EvaluationFailure::StalePlan))? =
        crate::fods::execution_trace::Trace::default();
    let correct = run
        .evaluate_cell(target(case.correct_row, case.correct_col))
        .map_err(aborted)?;
    if borrow_failed.get() {
        return Err(aborted(poirot_eval::EvaluationFailure::StalePlan));
    }
    if let Some(reason) = trace
        .try_borrow()
        .map_err(|_| aborted(poirot_eval::EvaluationFailure::StalePlan))?
        .failure("Correct")
    {
        return Err(reason);
    }
    if case.matrix.is_some() {
        return Err(failure(
            FailureKind::MatrixContext,
            format!(
                "Calc CSE matrix context unsupported; scalar actual={actual:?}; predicate={correct:?}"
            ),
        ));
    }
    if !matches!(correct, CellValue::Boolean(true) | CellValue::Number(1.0)) {
        let expected = if gaps::expected_is_error_flag(case) {
            None
        } else if let Some(formula) = &case.expected.formula {
            poirot_formula::parse_formula(formula)
                .ok()
                .filter(|expr| gaps::missing_functions(expr).is_empty())
                .and_then(|_| {
                    evaluate(
                        book,
                        case.sheet,
                        CellAddress {
                            row: case.expected.row,
                            col: case.expected.col,
                        },
                        formula,
                    )
                    .ok()
                })
        } else {
            Some(gaps::literal_expected(&case.expected.value))
        };
        let mut reason = failure(
            FailureKind::PredicateMismatch,
            format!(
                "actual={actual:?}; upstream Correct predicate={correct:?}; expected={:?}",
                case.expected
            ),
        );
        reason.gap_type = gaps::mismatch(&actual, expected.as_ref());
        *observed = Some(CheckObservation {
            actual,
            expected,
            predicate: correct,
        });
        if let Some(name) = actual_unresolved_name.as_deref() {
            reason.gap_type = GapKind::NameResolution;
            reason
                .message
                .push_str(&format!("; actual unresolved source name={name}"));
        }
        if let Some(name) = trace
            .try_borrow()
            .map_err(|_| aborted(poirot_eval::EvaluationFailure::StalePlan))?
            .unresolved_name()
        {
            reason.gap_type = GapKind::NameResolution;
            reason
                .message
                .push_str(&format!("; Correct unresolved source name={name}"));
        }
        return Err(reason);
    }
    Ok(())
}

pub(crate) fn fixture_results(
    plan: &FixturePlan,
    observer: &(impl Fn(&CaseResult) -> Result<(), Error> + Sync),
) -> Result<Vec<CaseResult>, Error> {
    fixture_observations(plan, observer)
        .map(|results| results.into_iter().map(|(result, _)| result).collect())
}

pub(crate) fn fixture_observations(
    plan: &FixturePlan,
    observer: &(impl Fn(&CaseResult) -> Result<(), Error> + Sync),
) -> Result<Vec<(CaseResult, Option<CheckObservation>)>, Error> {
    plan.cases
        .iter()
        .map(|case| {
            let mut observed = None;
            let check =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match &plan.workbook {
                    WorkbookPreparation::Ready(book) => check(
                        book,
                        plan.evaluation_plan.as_ref().ok_or_else(|| {
                            failure(
                                FailureKind::WorkbookContext,
                                "missing compiled workbook plan",
                            )
                        })?,
                        &plan.entry,
                        case,
                        &mut observed,
                    ),
                    WorkbookPreparation::Unsupported(reason) => Err(reason.clone()),
                }))
                .unwrap_or_else(|_| {
                    Err(failure(
                        FailureKind::EvaluatorPanic,
                        "ROI evaluator panicked",
                    ))
                });
            let status = match (&check, case.baseline) {
                (Err(reason), _) if reason.gap_type == GapKind::HarnessError => {
                    CaseStatus::HarnessError
                }
                (Ok(()), BaselineStatus::Pass) => CaseStatus::Pass,
                (Ok(()), BaselineStatus::Gap) => CaseStatus::Improvement,
                (Err(_), BaselineStatus::Pass) => CaseStatus::Regression,
                (Err(_), BaselineStatus::Gap) => CaseStatus::KnownGap,
            };
            let result = CaseResult {
                id: case.source.id.clone(),
                fixture: plan.entry.fixture.clone(),
                sheet: case.source.sheet_name.clone(),
                cell: case.source.cell.clone(),
                formula: case.source.formula.clone(),
                baseline: case.baseline,
                status,
                failure: check.err(),
            };
            observer(&result)?;
            Ok((result, observed))
        })
        .collect()
}

/// Evaluate a prepared fixture plan in bounded parallel workers and assemble ordered case results.
pub fn execute(plan: &RunPlan) -> Result<RunReport, Error> {
    execute_with_observer(plan, &|_| Ok(()))
}

/// Notify the observer immediately after each check, on the completing worker.
/// The final report retains source order regardless of completion order.
pub fn execute_with_observer(
    plan: &RunPlan,
    observer: &(impl Fn(&CaseResult) -> Result<(), Error> + Sync),
) -> Result<RunReport, Error> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(plan.jobs.get())
        .stack_size(16 * 1024 * 1024)
        .thread_name(|i| format!("fods-{i}"))
        .build()
        .map_err(|e| Error::Parallel(e.to_string()))?;
    let groups: Vec<Result<Vec<CaseResult>, Error>> = pool.install(|| {
        plan.fixtures
            .par_iter()
            .map(|fixture| fixture_results(fixture, observer))
            .collect()
    });
    let groups: Vec<Vec<CaseResult>> = groups.into_iter().collect::<Result<_, _>>()?;
    let cases: Vec<_> = groups.into_iter().flatten().collect();
    let mut summary = RunSummary {
        fixtures: plan.fixtures.len(),
        cases: cases.len(),
        no_case_fixtures: plan
            .fixtures
            .iter()
            .filter(|fixture| fixture.cases.is_empty())
            .count(),
        workers: plan.jobs.get(),
        ..RunSummary::default()
    };
    for case in &cases {
        if let Some(failure) = &case.failure {
            *summary.gap_types.entry(failure.gap_type).or_default() += 1;
        }
        match case.status {
            CaseStatus::Pass => summary.passed += 1,
            CaseStatus::KnownGap => summary.known_gaps += 1,
            CaseStatus::Regression => summary.regressions += 1,
            CaseStatus::Improvement => {
                summary.improvements += 1;
                summary.passed += 1;
            }
            CaseStatus::HarnessError => summary.harness_errors += 1,
        }
    }
    Ok(RunReport { summary, cases })
}
