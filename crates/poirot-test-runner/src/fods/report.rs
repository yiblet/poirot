//! Typed results and presentation. Expected results are never replaced by observations.
use crate::fods::corpus::BaselineStatus;
use crate::fods::gaps::GapKind;
use serde::{Deserialize, Serialize};

/// Execution-stage classification retained alongside detailed gap diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// Native workbook context is unsupported.
    WorkbookContext,
    /// Source formula parsing or translation failed.
    SourceSyntax,
    /// Consumed function/name is unavailable.
    UnsupportedName,
    /// Bounded evaluation ended with an operational failure.
    EvaluationAbort,
    /// Source requires unsupported matrix evaluation context.
    MatrixContext,
    /// Native check predicate did not pass.
    PredicateMismatch,
    /// Evaluator panicked instead of returning a bounded result.
    EvaluatorPanic,
}

/// Structured reason a source check did not produce a passing result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseFailure {
    /// Execution stage responsible for the failure.
    pub kind: FailureKind,
    /// Detailed stable semantic/capability category.
    pub gap_type: GapKind,
    /// Consumed unsupported function spellings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_functions: Vec<String>,
    /// Human-readable source and execution diagnostic.
    pub message: String,
}

/// Observed result compared with its retained baseline expectation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseStatus {
    /// Expected passing case still passes.
    Pass,
    /// Known gap remains failing.
    KnownGap,
    /// Previously passing case now fails.
    Regression,
    /// Previously known gap now passes.
    Improvement,
    /// Infrastructure failure prevents a reliable semantic result.
    HarnessError,
}

/// Source identity, baseline comparison, and optional diagnostic for one check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseResult {
    /// Stable source-case key.
    pub id: String,
    /// Owning native fixture identity.
    pub fixture: String,
    /// Source worksheet spelling.
    pub sheet: String,
    /// Source A1 check coordinate.
    pub cell: String,
    /// Evaluated translated source expression.
    pub formula: String,
    /// Recorded pass/gap expectation.
    pub baseline: BaselineStatus,
    /// Observed comparison with that baseline.
    pub status: CaseStatus,
    /// Structured failure details when the check does not pass.
    pub failure: Option<CaseFailure>,
}

/// Aggregated case outcomes and worker/fixture counts for one corpus run.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    /// Number of selected fixtures.
    pub fixtures: usize,
    /// Number of evaluated source checks.
    pub cases: usize,
    /// Count of expected passing checks that pass.
    pub passed: usize,
    /// Count of retained known gaps.
    pub known_gaps: usize,
    /// Count of newly failing checks.
    pub regressions: usize,
    /// Count of known gaps that now pass.
    pub improvements: usize,
    /// Count of infrastructure failures.
    pub harness_errors: usize,
    /// Selected fixtures containing no extracted checks.
    pub no_case_fixtures: usize,
    /// Worker count used by execution.
    pub workers: usize,
    /// Counts grouped by stable gap category.
    pub gap_types: std::collections::BTreeMap<GapKind, usize>,
}

/// Deterministically ordered case results and their aggregate summary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunReport {
    /// Aggregated run counts and diagnostic categories.
    pub summary: RunSummary,
    /// Case outcomes retained in source order.
    pub cases: Vec<CaseResult>,
}

impl RunReport {
    /// Whether the report satisfies its regression/strictness success contract.
    pub fn successful(&self, strict: bool) -> bool {
        self.summary.regressions == 0
            && self.summary.harness_errors == 0
            && (!strict || self.summary.known_gaps == 0)
    }
}

/// Tagged JSON-lines record distinguishing a case result from the final summary.
#[derive(Serialize)]
#[serde(tag = "record", rename_all = "snake_case")]
enum Record<'a> {
    Case {
        #[serde(flatten)]
        result: &'a CaseResult,
    },
    Summary {
        #[serde(flatten)]
        summary: &'a RunSummary,
        strict: bool,
        successful: bool,
    },
}

fn json_record(
    output: &mut impl std::io::Write,
    record: &Record<'_>,
) -> Result<(), crate::fods::Error> {
    serde_json::to_writer(&mut *output, record)
        .map_err(|error| crate::fods::Error::Output(std::io::Error::other(error)))?;
    writeln!(output).map_err(crate::fods::Error::Output)
}

/// Render one typed case outcome using the selected presentation format.
pub fn render_case(
    case: &CaseResult,
    output: &mut impl std::io::Write,
    format: crate::fods::cli::OutputFormat,
    strict: bool,
) -> Result<(), crate::fods::Error> {
    use crate::fods::{Error, cli::OutputFormat};
    match format {
        OutputFormat::Json => json_record(output, &Record::Case { result: case })?,
        OutputFormat::Text => {
            let status = match case.status {
                CaseStatus::Pass => "ok".to_owned(),
                CaseStatus::Improvement => "ok (improvement)".to_owned(),
                status => {
                    let label = match status {
                        CaseStatus::KnownGap => "known gap",
                        CaseStatus::Regression => "regression",
                        _ => "harness error",
                    };
                    let kind = case
                        .failure
                        .as_ref()
                        .map(|f| f.gap_type.label())
                        .unwrap_or("harness_error");
                    format!("FAILED ({label}: {kind})")
                }
            };
            writeln!(output, "test {} ... {status}", case.id).map_err(Error::Output)?;
            if (matches!(
                case.status,
                CaseStatus::Regression | CaseStatus::HarnessError
            ) || (strict && case.status == CaseStatus::KnownGap))
                && let Some(failure) = &case.failure
            {
                writeln!(
                    output,
                    "  {}!{}: {}",
                    case.sheet, case.cell, failure.message
                )
                .map_err(Error::Output)?;
            }
        }
    }
    output.flush().map_err(Error::Output)
}

/// Render the final typed run summary.
pub fn render_summary(
    report: &RunReport,
    output: &mut impl std::io::Write,
    format: crate::fods::cli::OutputFormat,
    strict: bool,
) -> Result<(), crate::fods::Error> {
    use crate::fods::{Error, cli::OutputFormat};
    match format {
        OutputFormat::Json => json_record(
            output,
            &Record::Summary {
                summary: &report.summary,
                strict,
                successful: report.successful(strict),
            },
        ),
        OutputFormat::Text => {
            let s = &report.summary;
            writeln!(output,
                "{} fixtures; {} cases: {} passed, {} known gaps, {} regressions, {} improvements, {} harness errors; {} without case layout; {} workers",
                s.fixtures, s.cases, s.passed, s.known_gaps, s.regressions, s.improvements,
                s.harness_errors, s.no_case_fixtures, s.workers).map_err(Error::Output)?;
            for (kind, count) in &s.gap_types {
                writeln!(output, "  {}: {count}", kind.label()).map_err(Error::Output)?;
            }
            Ok(())
        }
    }
}

/// Render an already collected report in stable source order.
pub fn render(
    report: &RunReport,
    output: &mut impl std::io::Write,
    format: crate::fods::cli::OutputFormat,
    strict: bool,
) -> Result<(), crate::fods::Error> {
    for case in &report.cases {
        render_case(case, output, format, strict)?;
    }
    render_summary(report, output, format, strict)
}
