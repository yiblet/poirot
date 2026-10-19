//! Refresh source assets and observation baseline without changing upstream expectations.
use crate::fods::{
    Error,
    cli::RefreshArgs,
    corpus::{Baseline, BaselineCase, BaselineStatus, CorpusManifest, FixtureEntry},
    execute,
    plan::{RunPlan, prepare_fixture},
    report::CaseStatus,
    xml::{SOURCE_COMMIT, model::Omission},
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

fn io(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_owned(),
        source,
    }
}

fn collect(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), Error> {
    for entry in std::fs::read_dir(directory).map_err(|e| io(directory, e))? {
        let entry = entry.map_err(|e| io(directory, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| io(&path, e))?;
        if kind.is_dir() {
            collect(&path, files)?;
        } else if kind.is_file() && path.extension().is_some_and(|ext| ext == "fods") {
            files.push(path);
        }
    }
    Ok(())
}

fn source_revision(source: &Path) -> Result<String, Error> {
    if source.join(".git").exists() {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(source)
            .args(["rev-parse", "HEAD"])
            .output()
            .map_err(|e| io(source, e))?;
        if !output.status.success() {
            return Err(Error::InvalidPlan(
                "cannot read source checkout revision".into(),
            ));
        }
        let diff = std::process::Command::new("git")
            .arg("-C")
            .arg(source)
            .args([
                "diff",
                "--quiet",
                SOURCE_COMMIT,
                "--",
                "sc/qa/unit/data/functions",
            ])
            .status()
            .map_err(|e| io(source, e))?;
        if !diff.success() {
            return Err(Error::InvalidPlan(
                "source formula fixtures must match the pinned checkout without local edits".into(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let path = source.join("revision.txt");
        std::fs::read_to_string(&path)
            .map(|s| s.trim().to_owned())
            .map_err(|e| io(&path, e))
    }
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<(), Error> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::Metadata {
        path: path.to_owned(),
        source,
    })?;
    std::fs::write(path, bytes).map_err(|e| io(path, e))
}

/// Extract pinned source fixtures and rebuild checked corpus metadata.
pub fn run(args: &RefreshArgs) -> Result<bool, Error> {
    if source_revision(&args.source)? != SOURCE_COMMIT {
        return Err(Error::InvalidPlan(format!(
            "source must be pinned at {SOURCE_COMMIT}"
        )));
    }
    // An existing destination must be a corpus, never an arbitrary directory.
    if args.corpus.exists() {
        if args.corpus.is_symlink() {
            return Err(Error::InvalidPlan(
                "refresh destination must not be a symlink".into(),
            ));
        }
        crate::fods::corpus::Corpus::open(&args.corpus)?;
        for entry in std::fs::read_dir(&args.corpus).map_err(|e| io(&args.corpus, e))? {
            let entry = entry.map_err(|e| io(&args.corpus, e))?;
            if !matches!(
                entry.file_name().to_str(),
                Some(
                    "fixtures"
                        | "manifest.json"
                        | "classifications.json"
                        | "README.md"
                        | "COPYING.MPL"
                )
            ) {
                return Err(Error::InvalidPlan(format!(
                    "refresh destination contains unrelated content: {}",
                    entry.path().display()
                )));
            }
        }
    }
    let input = args.source.join("sc/qa/unit/data/functions");
    let mut paths = Vec::new();
    collect(&input, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(Error::InvalidPlan(
            "source contains no FODS formula fixtures".into(),
        ));
    }
    let parent = args
        .corpus
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Error::InvalidPlan(e.to_string()))?
        .as_nanos();
    let staging = parent.join(format!(".fods-refresh-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&staging).map_err(|e| io(&staging, e))?;
    let result = refresh_staged(args, &input, &paths, &staging, stamp);
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| io(&staging, e))?;
    }
    result
}

fn refresh_staged(
    args: &RefreshArgs,
    input: &Path,
    paths: &[PathBuf],
    staging: &Path,
    stamp: u128,
) -> Result<bool, Error> {
    let mut manifest = CorpusManifest {
        source_repository: "https://github.com/LibreOffice/core".into(),
        source_commit: SOURCE_COMMIT.into(),
        license: "MPL-2.0".into(),
        fixtures: Vec::new(),
        case_count: 0,
        omissions: Vec::new(),
    };
    let mut baseline = HashMap::new();
    let mut fixtures = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(input)
            .map_err(|e| Error::InvalidPlan(e.to_string()))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        let source = format!("sc/qa/unit/data/functions/{name}");
        let bytes = std::fs::read(path).map_err(|e| io(path, e))?;
        let data =
            crate::fods::xml::extract_bytes(&bytes, &source).map_err(|message| Error::Fods {
                path: path.clone(),
                message,
            })?;
        let entry = FixtureEntry {
            fixture: name.clone(),
            id: crate::fods::cases::source_key(&source),
            source: source.clone(),
            sha256: data.sha256.clone(),
            cases: data.cases.len(),
        };
        for (index, case) in data.cases.iter().enumerate() {
            let record = BaselineCase {
                id: case.id.clone(),
                fixture: name.clone(),
                index,
                sheet: Some(case.sheet_name.clone()),
                cell: Some(case.cell.clone()),
                classification: BaselineStatus::Gap,
                reason: None,
                gap_type: None,
            };
            if baseline.insert(record.id.clone(), record).is_some() {
                return Err(Error::InvalidPlan("source case IDs collide".into()));
            }
        }
        manifest.case_count += entry.cases;
        manifest
            .omissions
            .extend(data.omissions.iter().cloned().map(|mut o| {
                o.source = Some(source.clone());
                o
            }));
        if data.cases.is_empty() {
            manifest.omissions.push(Omission {
                kind: crate::fods::xml::model::OmissionKind::UnsupportedConstruct,
                source: Some(source),
                sheet: None,
                cell: None,
                reason: "no explicit Function/Expected/Correct formula cases".into(),
            });
        }
        fixtures.push(prepare_fixture(entry.clone(), data, &baseline)?);
        manifest.fixtures.push(entry);
        let destination = staging.join("fixtures").join(relative);
        std::fs::create_dir_all(destination.parent().ok_or_else(|| {
            io(
                &destination,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "destination has no parent",
                ),
            )
        })?)
        .map_err(|e| io(&destination, e))?;
        std::fs::write(&destination, &bytes).map_err(|e| io(&destination, e))?;
    }
    let report = execute::execute(&RunPlan {
        source_commit: SOURCE_COMMIT.into(),
        fixtures,
        jobs: args.jobs,
    })?;
    if report.summary.harness_errors != 0 {
        return Err(Error::InvalidPlan(
            "evaluator panic prevents publishing a refresh baseline".into(),
        ));
    }
    let mut records = Vec::with_capacity(report.cases.len());
    for case in report.cases {
        let mut record = baseline
            .remove(&case.id)
            .ok_or_else(|| Error::InvalidPlan("missing source case during refresh".into()))?;
        record.classification = if matches!(case.status, CaseStatus::Pass | CaseStatus::Improvement)
        {
            BaselineStatus::Pass
        } else {
            BaselineStatus::Gap
        };
        record.gap_type = case.failure.as_ref().map(|failure| failure.gap_type);
        record.reason = case.failure.map(|failure| failure.message);
        records.push(record);
    }
    write_json(&staging.join("manifest.json"), &manifest)?;
    write_json(
        &staging.join("classifications.json"),
        &Baseline {
            source_commit: SOURCE_COMMIT.into(),
            cases: records,
        },
    )?;
    for name in ["COPYING.MPL", "README.md"] {
        let current = args.corpus.join(name);
        if current.is_file() {
            std::fs::copy(&current, staging.join(name)).map_err(|e| io(&current, e))?;
        } else if name == "COPYING.MPL" {
            let source = args.source.join(name);
            std::fs::copy(&source, staging.join(name)).map_err(|e| io(&source, e))?;
        }
    }
    let backup = staging.with_file_name(format!(".fods-backup-{}-{stamp}", std::process::id()));
    let existed = args.corpus.exists();
    if existed {
        std::fs::rename(&args.corpus, &backup).map_err(|e| io(&args.corpus, e))?;
    }
    if let Err(error) = std::fs::rename(staging, &args.corpus) {
        if existed {
            std::fs::rename(&backup, &args.corpus).map_err(|e| io(&backup, e))?;
        }
        return Err(io(&args.corpus, error));
    }
    if existed {
        std::fs::remove_dir_all(&backup).map_err(|e| io(&backup, e))?;
    }
    println!(
        "Refreshed {} original FODS fixtures and {} cases: {} passing, {} known gaps. Review the baseline diff before committing.",
        manifest.fixtures.len(),
        manifest.case_count,
        report.summary.passed,
        report.summary.known_gaps
    );
    Ok(true)
}
