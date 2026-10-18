//! Add categories to known gaps while keeping baseline verdicts unchanged.
use crate::fods::{
    Error,
    cli::ClassifyArgs,
    corpus::{Baseline, BaselineStatus},
    execute,
    plan::{self, RunOptions},
};

/// Classify existing corpus gaps and persist their diagnostic categories.
pub fn run(args: &ClassifyArgs) -> Result<bool, Error> {
    let path = args.corpus.join("classifications.json");
    if !path.is_file() {
        return Err(Error::InvalidPlan("native classify requires a standalone classifications.json; use baseline update for the combined checked-in corpus".into()));
    }
    let original = std::fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    let plan = plan::build(&RunOptions {
        corpus: args.corpus.clone(),
        filter: None,
        jobs: args.jobs,
    })?;
    let report = execute::execute(&plan)?;
    if report.summary.harness_errors != 0 {
        return Err(Error::InvalidPlan(
            "harness errors prevent gap classification".into(),
        ));
    }
    let mut baseline: Baseline =
        serde_json::from_slice(&original).map_err(|source| Error::Metadata {
            path: path.clone(),
            source,
        })?;
    let results: std::collections::HashMap<_, _> =
        report.cases.iter().map(|case| (&case.id, case)).collect();
    let mut labeled = 0;
    for case in &mut baseline.cases {
        if case.classification == BaselineStatus::Gap {
            let result = results.get(&case.id).ok_or_else(|| {
                Error::InvalidPlan(format!("no classification result for {}", case.id))
            })?;
            case.gap_type = result.failure.as_ref().map(|failure| failure.gap_type);
            labeled += usize::from(case.gap_type.is_some());
        }
    }
    // Refuse to overwrite metadata edited while classification was running.
    if std::fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })? != original
    {
        return Err(Error::InvalidPlan(
            "classification baseline changed during the run".into(),
        ));
    }
    let bytes = serde_json::to_vec(&baseline).map_err(|source| Error::Metadata {
        path: path.clone(),
        source,
    })?;
    let temporary = args
        .corpus
        .join(format!(".classifications-{}.json", std::process::id()));
    let result = (|| {
        std::fs::write(&temporary, bytes).map_err(|source| Error::Io {
            path: temporary.clone(),
            source,
        })?;
        std::fs::rename(&temporary, &path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })
    })();
    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    result?;
    println!(
        "Labeled {labeled} known gaps; pass/gap verdicts and upstream expectations are unchanged."
    );
    Ok(true)
}
