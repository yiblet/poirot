//! One command-line interface for discovery, execution and saved reports.
use crate::{
    Error, baseline, invalid,
    model::{Change, digest},
    report::{self, Format},
    runner::{self, Options},
    selection::Selection,
};
use clap::{Args, Parser, Subcommand};
use std::{collections::BTreeMap, io::Write, num::NonZeroUsize, path::PathBuf};

/// Command-line entry point for unified discovery, execution, and saved-result tools.
/// With no subcommand it executes a run using the flattened run arguments.
#[derive(Debug, Parser)]
#[command(
    name = "poirot-test-runner",
    about = "Grouped workbook tests compared with a checked-in baseline",
    version,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub run: RunArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Run(RunArgs),
    /// Preview the selection without compiling or evaluating workbooks.
    List(ListArgs),
    /// Inspect a single saved result without evaluating it again.
    Show {
        case: String,
        #[arg(long)]
        results: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Generate a self-contained searchable HTML viewer from a saved JSONL run.
    Report {
        results: PathBuf,
        #[arg(long)]
        html: PathBuf,
    },
    /// Produce a reviewable baseline candidate; normal runs never update it.
    Baseline {
        #[command(subcommand)]
        command: BaselineCommand,
    },
    /// Source-specific inspection and extraction tools.
    Fods {
        #[command(subcommand)]
        command: FodsCommand,
    },
}

/// Corpus roots, baseline path, and case selection shared by the run and list commands.
/// Converting these arguments to runner options keeps discovery consistent across both commands.
#[derive(Debug, Args)]
pub struct Inputs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, default_value_os_t = crate::package_root().join("corpus/fods"))]
    pub fods_corpus: PathBuf,
    /// JSON suite file or directory; suite IDs are relative to this root.
    #[arg(long, default_value_os_t = crate::package_root().join("corpus/json"))]
    pub json_corpus: PathBuf,
    #[arg(long, default_value_os_t = crate::package_root().join("baseline.json"))]
    pub baseline: PathBuf,
}

impl Inputs {
    fn options(&self) -> Options {
        Options {
            fods_corpus: self.fods_corpus.clone(),
            json_corpus: self.json_corpus.clone(),
            baseline: self.baseline.clone(),
            selection: self.selection.clone(),
            ..Options::default()
        }
    }
}

/// Command-line controls for executing selected cases and presenting their results.
/// They extend shared discovery inputs with worker count, skip rules, strictness, and output choices.
#[derive(Debug, Args)]
pub struct RunArgs {
    #[command(flatten)]
    pub inputs: Inputs,
    #[arg(long, default_value_t = crate::fods::cli::default_jobs())]
    pub jobs: NonZeroUsize,
    /// Also fail unchanged known mismatches, blockers and explicit skips.
    #[arg(long)]
    pub strict: bool,
    #[arg(long)]
    pub skip_file: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    #[arg(long, conflicts_with = "quiet")]
    pub verbose: bool,
    #[arg(long)]
    pub quiet: bool,
}

/// Command-line controls for previewing the selected test inventory without evaluation.
/// The detailed form includes individual stable case IDs for later execution or inspection.
#[derive(Debug, Args)]
pub struct ListArgs {
    #[command(flatten)]
    pub inputs: Inputs,
    /// Include individual stable IDs and human-readable case descriptions.
    #[arg(long = "cases")]
    pub detailed_cases: bool,
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

#[derive(Debug, Subcommand)]
pub enum BaselineCommand {
    /// Write a new candidate file; existing files are never overwritten.
    Update {
        #[arg(long)]
        results: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        reason: String,
        #[arg(long, default_value_os_t = crate::package_root().join("baseline.json"))]
        baseline: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum FodsCommand {
    Inspect(crate::fods::cli::InspectArgs),
    /// Extract to a separate corpus before reviewing and importing the result.
    Refresh {
        #[arg(long)]
        source: PathBuf,
        #[arg(long)]
        corpus: PathBuf,
        #[arg(long, default_value_t = crate::fods::cli::default_jobs())]
        jobs: NonZeroUsize,
    },
}

impl Cli {
    pub fn execute(self, output: &mut impl Write) -> Result<bool, Error> {
        match self.command.unwrap_or(Command::Run(self.run)) {
            Command::Run(args) => {
                let mut options = args.inputs.options();
                options.jobs = args.jobs;
                options.strict = args.strict;
                options.skips = args.skip_file;
                Ok(
                    runner::run(&options, output, args.format, args.verbose, args.quiet)?
                        .summary
                        .successful,
                )
            }
            Command::List(args) => {
                let cases = runner::list(&args.inputs.options())?;
                if args.format == Format::Json {
                    for case in &cases {
                        serde_json::to_writer(&mut *output, case)?;
                        writeln!(output)?;
                    }
                } else {
                    let mut groups = BTreeMap::new();
                    for case in &cases {
                        groups
                            .entry((case.source, &case.group, &case.suite))
                            .or_insert_with(Vec::new)
                            .push(case);
                    }
                    for ((source, group, suite), cases) in groups {
                        writeln!(
                            output,
                            "{source} / {} / {}  {} cases",
                            report::single_line(group),
                            report::single_line(suite),
                            cases.len()
                        )?;
                        if args.detailed_cases {
                            for case in cases {
                                writeln!(
                                    output,
                                    "  {}\n    {}",
                                    report::single_line(&case.id),
                                    report::single_line(&case.name)
                                )?;
                            }
                        }
                    }
                    writeln!(output, "\n{} selected cases", cases.len())?;
                }
                Ok(true)
            }
            Command::Show {
                case,
                results,
                format,
            } => {
                let report = report::read(&results)?;
                let result = report
                    .cases
                    .iter()
                    .find(|result| result.case.id == case)
                    .ok_or_else(|| invalid(format!("case not present in saved report: {case}")))?;
                if format == Format::Json {
                    serde_json::to_writer(&mut *output, result)?;
                    writeln!(output)?;
                } else {
                    report::detail(output, result)?;
                    writeln!(
                        output,
                        "\n{}",
                        serde_json::to_string_pretty(&result.details)?
                    )?;
                }
                Ok(true)
            }
            Command::Report { results, html } => {
                std::fs::write(&html, report::html(&report::read(&results)?)?)?;
                writeln!(output, "Saved {}", html.display())?;
                Ok(true)
            }
            Command::Baseline {
                command:
                    BaselineCommand::Update {
                        results,
                        output: candidate,
                        reason,
                        baseline: current,
                    },
            } => {
                let report = report::read(&results)?;
                if digest(&std::fs::read(current)?) != report.header.baseline_digest {
                    return Err(invalid(
                        "baseline changed since this run; rerun before producing a candidate",
                    ));
                }
                let baseline = baseline::candidate(&report, &reason)?;
                baseline::write_candidate(&candidate, &baseline)?;
                writeln!(
                    output,
                    "Saved candidate {} ({} entries). Review its diff before replacing baseline.json.",
                    candidate.display(),
                    baseline.cases.len()
                )?;
                for (change, count) in &report.summary.changes {
                    if *change != Change::Unchanged {
                        writeln!(output, "  {count} {change:?}")?;
                    }
                }
                Ok(true)
            }
            Command::Fods {
                command: FodsCommand::Inspect(args),
            } => Ok(crate::fods::commands::inspect::run(&args)?),
            Command::Fods {
                command:
                    FodsCommand::Refresh {
                        source,
                        corpus,
                        jobs,
                    },
            } => {
                if corpus == crate::package_root().join("corpus/fods") {
                    return Err(invalid(
                        "extract to a separate corpus; the shared checked-in baseline is updated only through baseline update",
                    ));
                }
                Ok(crate::fods::commands::refresh::run(
                    &crate::fods::cli::RefreshArgs {
                        source,
                        corpus,
                        jobs,
                    },
                )?)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn case_selection_and_detailed_listing_have_distinct_argument_ids() {
        Cli::command().debug_assert();
        let cli = Cli::try_parse_from(["runner", "list", "--case", "json:suite.json:a", "--cases"])
            .unwrap();
        let Some(Command::List(args)) = cli.command else {
            panic!("expected list");
        };
        assert!(args.detailed_cases);
        assert_eq!(args.inputs.selection.cases, vec!["json:suite.json:a"]);
    }
}
