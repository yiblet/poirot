//! Add categories to known gaps while keeping baseline verdicts unchanged.
use crate::fods::{
    FodsError,
    cli::ClassifyArgs,
    corpus::{Baseline, BaselineStatus},
    execute,
    plan::{self, RunOptions},
};

/// Classify existing corpus gaps and persist their diagnostic categories.
pub fn run(args: &ClassifyArgs) -> Result<bool, FodsError> {
    let path = args.corpus.join("classifications.json");
    if !path.is_file() {
        return Err(FodsError::MissingStandaloneBaseline);
    }
    let original = std::fs::read(&path).map_err(|source| FodsError::Io {
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
        return Err(FodsError::ClassificationHarnessErrors);
    }
    let mut baseline: Baseline =
        serde_json::from_slice(&original).map_err(|source| FodsError::Metadata {
            path: path.clone(),
            source,
        })?;
    let results: std::collections::HashMap<_, _> =
        report.cases.iter().map(|case| (&case.id, case)).collect();
    let mut labeled = 0;
    for case in &mut baseline.cases {
        if case.classification == BaselineStatus::Gap {
            let result =
                results
                    .get(&case.id)
                    .ok_or_else(|| FodsError::ClassificationCaseMissing {
                        case: case.id.clone(),
                    })?;
            case.gap_type = result.failure.as_ref().map(|failure| failure.gap_type);
            labeled += usize::from(case.gap_type.is_some());
        }
    }
    // Refuse to overwrite metadata edited while classification was running.
    if std::fs::read(&path).map_err(|source| FodsError::Io {
        path: path.clone(),
        source,
    })? != original
    {
        return Err(FodsError::ClassificationBaselineChanged);
    }
    let bytes = serde_json::to_vec(&baseline).map_err(|source| FodsError::Metadata {
        path: path.clone(),
        source,
    })?;
    let temporary = args
        .corpus
        .join(format!(".classifications-{}.json", std::process::id()));
    let result = (|| {
        std::fs::write(&temporary, bytes).map_err(|source| FodsError::Io {
            path: temporary.clone(),
            source,
        })?;
        std::fs::rename(&temporary, &path).map_err(|source| FodsError::Io {
            path: path.clone(),
            source,
        })
    })();
    if temporary.exists() {
        std::fs::remove_file(&temporary).map_err(|source| FodsError::Io {
            path: temporary.clone(),
            source,
        })?;
    }
    result?;
    println!(
        "Labeled {labeled} known gaps; pass/gap verdicts and upstream expectations are unchanged."
    );
    Ok(true)
}
