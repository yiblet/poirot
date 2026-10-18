use crate::json::{Status, parse_suite, run_suite};
use clap::{Parser, ValueEnum};
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Reusable command-line interface for the JSON scenario runner.
#[derive(Debug, Parser)]
#[command(about = "Check JSON workbook scenarios in normal and overlay evaluation")]
pub struct Cli {
    /// A JSON suite or a directory of JSON suites (searched recursively).
    #[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/corpus/json"))]
    pub corpus: PathBuf,
    /// Select scenario names containing this substring.
    #[arg(long)]
    pub filter: Option<String>,
    /// Fail on documented known gaps as well as regressions and stale gaps.
    #[arg(long)]
    pub strict: bool,
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

fn files(path: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    if path.is_file() {
        out.push(path.to_owned());
    } else {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            // Do not follow directory symlinks into recursive cycles.
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                files(&path, out)?;
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }
    Ok(())
}

#[derive(Default, Serialize)]
struct Summary {
    scenarios: usize,
    passed: usize,
    known_gaps: usize,
    unexpected_passes: usize,
    failed: usize,
    harness_errors: usize,
}

impl Cli {
    /// Write text or JSONL results. Return false when the selected run fails.
    pub fn run(&self, out: &mut impl Write) -> Result<bool, Box<dyn std::error::Error>> {
        let mut paths = Vec::new();
        files(&self.corpus, &mut paths)?;
        paths.sort();
        let mut summary = Summary::default();
        let mut success = true;
        for path in paths {
            let bytes = std::fs::read(&path)?;
            if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'[') {
                for case in super::specs::parse(&bytes)? {
                    if self
                        .filter
                        .as_deref()
                        .is_some_and(|filter| !case.name.contains(filter))
                    {
                        continue;
                    }
                    let observed = super::specs::observe(&case)?;
                    let passed = observed.failures.is_empty() && observed.mode_errors.is_empty();
                    summary.scenarios += 1;
                    if passed {
                        summary.passed += 1;
                    } else {
                        summary.failed += 1;
                    }
                    success &= passed;
                    match self.format {
                        OutputFormat::Text => writeln!(
                            out,
                            "test {}::{} ... {}",
                            path.display(),
                            case.name,
                            if passed { "Passed" } else { "Failed" }
                        )?,
                        OutputFormat::Json => {
                            serde_json::to_writer(
                                &mut *out,
                                &serde_json::json!({"type":"scenario","suite":path,"result":{"name":case.name,"status":if passed {"passed"} else {"failed"},"checks":observed.checks,"mode_errors":observed.mode_errors}}),
                            )?;
                            writeln!(out)?;
                        }
                    }
                }
                continue;
            }
            let run = (|| -> Result<_, String> {
                let text = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
                let suite = parse_suite(text)?;
                run_suite(&suite, self.filter.as_deref())
            })();
            let results = match run {
                Ok(results) => results,
                Err(error) => {
                    success = false;
                    summary.harness_errors += 1;
                    match self.format {
                        OutputFormat::Text => {
                            writeln!(out, "{}: HARNESS ERROR: {error}", path.display())?
                        }
                        OutputFormat::Json => {
                            serde_json::to_writer(
                                &mut *out,
                                &serde_json::json!({"type":"harness_error", "suite":path, "error":error}),
                            )?;
                            writeln!(out)?;
                        }
                    }
                    continue;
                }
            };
            for result in results {
                summary.scenarios += 1;
                success &= !result.fails(self.strict);
                match result.status {
                    Status::Passed => summary.passed += 1,
                    Status::KnownGap => summary.known_gaps += 1,
                    Status::UnexpectedPass => summary.unexpected_passes += 1,
                    Status::Failed => summary.failed += 1,
                    Status::HarnessError => summary.harness_errors += 1,
                }
                match self.format {
                    OutputFormat::Text => {
                        writeln!(
                            out,
                            "test {}::{} ... {:?}",
                            path.display(),
                            result.name,
                            result.status
                        )?;
                        if result.status != Status::Passed {
                            writeln!(
                                out,
                                "  outputs_equal={}, changes_equal={}",
                                result.outputs_equal, result.changes_equal
                            )?;
                            for mode in [&result.normal, &result.overlay] {
                                if let Some(error) = &mode.error {
                                    writeln!(out, "  {}: {error}", mode.mode)?;
                                }
                                for check in &mode.checks {
                                    if check.status != Status::Passed {
                                        writeln!(
                                            out,
                                            "  {} action {} {}: expected {:?}, actual {:?}; {}",
                                            mode.mode,
                                            check.action,
                                            check.cell,
                                            check.expected,
                                            check.actual,
                                            check.detail
                                        )?;
                                    }
                                }
                            }
                            if !result.changes_equal {
                                writeln!(
                                    out,
                                    "  normal changes: {:?}\n  overlay changes: {:?}",
                                    result.normal.changes, result.overlay.changes
                                )?;
                            }
                        }
                    }
                    OutputFormat::Json => {
                        serde_json::to_writer(
                            &mut *out,
                            &serde_json::json!({"type":"scenario", "suite":path, "result":result}),
                        )?;
                        writeln!(out)?;
                    }
                }
            }
        }
        if summary.scenarios == 0 {
            success = false;
        }
        match self.format {
            OutputFormat::Text => writeln!(
                out,
                "{} scenarios: {} passed, {} known gaps, {} unexpected passes, {} failed, {} harness errors",
                summary.scenarios,
                summary.passed,
                summary.known_gaps,
                summary.unexpected_passes,
                summary.failed,
                summary.harness_errors
            )?,
            OutputFormat::Json => {
                serde_json::to_writer(
                    &mut *out,
                    &serde_json::json!({"type":"summary", "success":success, "summary":summary}),
                )?;
                writeln!(out)?;
            }
        }
        Ok(success)
    }
}
