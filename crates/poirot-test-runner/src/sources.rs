//! Adapters own source interpretation; scheduling and reporting use one interface.
use crate::{
    Error, invalid,
    model::{self, CaseMetadata, CaseResult, Change, Observation, Outcome, Source},
};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
    sync::Arc,
};

pub(crate) trait TestSuite: Send + Sync {
    fn cases(&self) -> &[CaseMetadata];
    fn run(&self, selected: &BTreeSet<String>) -> Result<Vec<CaseResult>, Error>;
}

/// One extracted native FODS fixture exposed through the unified test-suite interface.
/// It prepares selected source checks with their pinned baseline and translates native
/// execution diagnostics into comparable runner observations.
pub(crate) struct FodsSuite {
    cases: Vec<CaseMetadata>,
    source: crate::fods::xml::model::FodsWorkbook,
    entry: crate::fods::corpus::FixtureEntry,
    baseline: Arc<HashMap<String, crate::fods::corpus::BaselineCase>>,
}

impl TestSuite for FodsSuite {
    fn cases(&self) -> &[CaseMetadata] {
        &self.cases
    }
    fn run(&self, selected: &BTreeSet<String>) -> Result<Vec<CaseResult>, Error> {
        let mut plan = crate::fods::plan::prepare_fixture(
            self.entry.clone(),
            self.source.clone(),
            &self.baseline,
        )?;
        plan.cases
            .retain(|case| selected.contains(&format!("fods:{}", case.source.id)));
        let results = crate::fods::execute::fixture_observations(&plan, &|_| Ok(()))?;
        results
            .into_iter()
            .map(|(native, observed)| {
                let id = format!("fods:{}", native.id);
                let case = self
                    .cases
                    .iter()
                    .find(|case| case.id == id)
                    .ok_or_else(|| invalid(format!("unknown executed FODS case: {id}")))?
                    .clone();
                let (observation, mut reason) = if let Some(failure) = &native.failure {
                    let category = failure.gap_type.label().to_owned();
                    let outcome = match failure.gap_type {
                        crate::fods::gaps::GapKind::HarnessError => Outcome::HarnessError,
                        crate::fods::gaps::GapKind::MissingFunction
                        | crate::fods::gaps::GapKind::UnsupportedFormulaSyntax
                        | crate::fods::gaps::GapKind::UnsupportedWorkbookContext
                        | crate::fods::gaps::GapKind::UnsupportedArrayContext
                        | crate::fods::gaps::GapKind::ResourceLimit
                        | crate::fods::gaps::GapKind::UnsupportedCapability
                        | crate::fods::gaps::GapKind::UnsupportedCheck
                        | crate::fods::gaps::GapKind::SourceException => Outcome::Blocked,
                        _ => Outcome::Mismatch,
                    };
                    (
                        Observation {
                            outcome,
                            category: Some(category),
                            fingerprint: Some(model::digest(&serde_json::to_vec(failure)?)),
                        },
                        failure.message.clone(),
                    )
                } else {
                    (Observation::passed(), String::new())
                };
                if observation.outcome == Outcome::Mismatch
                    && observation.category.as_deref() != Some("name_resolution")
                    && let Some(observed) = observed
                {
                    reason = format!(
                        "Expected: {}\nActual: {:?}\nCorrect predicate: {:?}",
                        observed
                            .expected
                            .map(|value| format!("{value:?}"))
                            .unwrap_or_else(|| "upstream Correct predicate TRUE".into()),
                        observed.actual,
                        observed.predicate
                    );
                }
                Ok(CaseResult {
                    case,
                    observation,
                    reason,
                    details: serde_json::to_value(native)?,
                    change: Change::Unchanged,
                    baseline: None,
                })
            })
            .collect()
    }
}

pub(crate) fn fods(root: &Path) -> Result<Vec<Box<dyn TestSuite>>, Error> {
    let corpus = crate::fods::corpus::Corpus::open(root)?;
    let baseline = Arc::new(corpus.baseline);
    corpus
        .manifest
        .fixtures
        .into_iter()
        .map(|entry| {
            let source = crate::fods::corpus::load_fixture(root, &entry)?;
            let group = entry
                .fixture
                .split('/')
                .next()
                .unwrap_or("other")
                .to_owned();
            let cases = source
                .cases
                .iter()
                .enumerate()
                .map(|(index, case)| CaseMetadata {
                    id: format!("fods:{}", case.id),
                    source: Source::Fods,
                    group: group.clone(),
                    suite: entry.fixture.clone(),
                    name: case.formula.clone(),
                    location: format!("{}!{}", case.sheet_name, case.cell),
                    formula: Some(case.formula.clone()),
                    authority:
                        "LibreOffice upstream Correct predicate; not native Excel verification"
                            .into(),
                    evidence: vec![format!("{} @ {}", entry.source, source.source_commit)],
                    input_digest: entry.sha256.clone(),
                    fods: Some(model::FodsBinding {
                        index,
                        sheet: case.sheet_name.clone(),
                        cell: case.cell.clone(),
                    }),
                })
                .collect();
            Ok(Box::new(FodsSuite {
                cases,
                source,
                entry,
                baseline: baseline.clone(),
            }) as Box<dyn TestSuite>)
        })
        .collect()
}

/// An authored JSON scenario suite exposed through the unified runner.
/// Selected scenarios retain their normal-versus-overlay checks while their results
/// are converted into the shared observation and baseline-comparison model.
struct JsonSuite {
    cases: Vec<CaseMetadata>,
    suite: crate::json::schema::Suite,
}

impl TestSuite for JsonSuite {
    fn cases(&self) -> &[CaseMetadata] {
        &self.cases
    }
    fn run(&self, selected: &BTreeSet<String>) -> Result<Vec<CaseResult>, Error> {
        let names: BTreeSet<_> = self
            .cases
            .iter()
            .filter(|case| selected.contains(&case.id))
            .map(|case| case.name.as_str())
            .collect();
        let results = crate::json::run_selected(&self.suite, &names).map_err(invalid)?;
        results.into_iter().map(|native| {
            let case = self.cases.iter().find(|case| case.name == native.name)
                .ok_or_else(|| invalid("unknown executed JSON scenario"))?.clone();
            let details = serde_json::to_value(&native)?;
            let category = match native.status {
                crate::json::Status::Passed => None,
                crate::json::Status::KnownGap => Some("authored_known_gap"),
                crate::json::Status::UnexpectedPass => Some("stale_annotation"),
                crate::json::Status::HarnessError => Some("evaluation_error"),
                crate::json::Status::Failed if !native.changes_equal => Some("change_list_mismatch"),
                crate::json::Status::Failed if !native.outputs_equal => Some("mode_mismatch"),
                crate::json::Status::Failed => Some("assertion_mismatch"),
            };
            let mut lines = Vec::new();
            let mut facts = Vec::new();
            for mode in [&native.normal, &native.overlay] {
                if let Some(error) = &mode.error { lines.push(format!("{}: {error}", mode.mode)); }
                for check in &mode.checks {
                    if check.status != crate::json::Status::Passed {
                        lines.push(format!("{} action {} {}: expected {:?}; actual {:?}; {}", mode.mode,
                            check.action, check.cell, check.expected, check.actual, check.detail));
                        facts.push(serde_json::to_value(check)?);
                    }
                }
            }
            if !native.changes_equal { lines.push("normal/overlay raw ordered change lists differ".into()); }
            if !native.outputs_equal { lines.push("normal/overlay typed observations differ".into()); }
            let outcome = match native.status {
                crate::json::Status::Passed => Outcome::Passed,
                crate::json::Status::HarnessError => Outcome::HarnessError,
                _ => Outcome::Mismatch,
            };
            let fingerprint = if category.is_none() { None } else {
                Some(json_observation_fingerprint(serde_json::json!({
                    "checks": facts, "normal_error":native.normal.error, "overlay_error":native.overlay.error,
                    "changes": if native.changes_equal { serde_json::Value::Null } else { serde_json::json!([native.normal.changes, native.overlay.changes]) }
                }))?)
            };
            Ok(CaseResult { case, observation: Observation { outcome, category: category.map(str::to_owned), fingerprint },
                reason: lines.join("\n"), details, change: Change::Unchanged, baseline: None })
        }).collect()
    }
}

/// One evidence-backed Excel specification case exposed as a unified runner suite.
/// Its step assertions become mismatch observations, while execution errors are reported as blockers.
struct ExcelSpecSuite {
    cases: Vec<CaseMetadata>,
    case: crate::json::specs::Case,
}
impl TestSuite for ExcelSpecSuite {
    fn cases(&self) -> &[CaseMetadata] {
        &self.cases
    }
    fn run(&self, selected: &BTreeSet<String>) -> Result<Vec<CaseResult>, Error> {
        let Some(case) = self
            .cases
            .first()
            .filter(|case| selected.contains(&case.id))
        else {
            return Ok(Vec::new());
        };
        let observed = crate::json::specs::observe(&self.case).map_err(invalid)?;
        let observation = if !observed.mode_errors.is_empty() {
            Observation {
                outcome: Outcome::Blocked,
                category: Some("excel_specification_execution".into()),
                fingerprint: Some(model::digest(&serde_json::to_vec(&observed.mode_errors)?)),
            }
        } else if !observed.failures.is_empty() {
            Observation {
                outcome: Outcome::Mismatch,
                category: Some("excel_specification_assertion".into()),
                fingerprint: Some(model::digest(&serde_json::to_vec(&observed.failures)?)),
            }
        } else {
            Observation::passed()
        };
        let reason = observed
            .mode_errors
            .iter()
            .cloned()
            .chain(observed.failures.iter().map(ToString::to_string))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(vec![CaseResult {
            case: case.clone(),
            observation,
            reason,
            details: serde_json::json!({"schema":"excel_specification","category":self.case.category,"checks":observed.checks,"mode_errors":observed.mode_errors}),
            change: Change::Unchanged,
            baseline: None,
        }])
    }
}

fn files(root: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), Error> {
    if root.is_symlink() {
        return Err(invalid("corpus roots must not be symlinks"));
    }
    if root.is_file() {
        out.push(root.to_owned());
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
    Ok(())
}

pub(crate) fn json(root: &Path) -> Result<Vec<Box<dyn TestSuite>>, Error> {
    let groups: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(crate::package_root().join("groups.json"))?)?;
    let mut paths = Vec::new();
    files(root, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|path| -> Result<Vec<Box<dyn TestSuite>>, Error> {
            let bytes = std::fs::read(&path)?;
            let relative = path.strip_prefix(if root.is_file() {
                root.parent().unwrap_or(root)
            } else {
                root
            })?;
            let input_digest = model::digest(&bytes);
            let name = relative.to_string_lossy().replace('\\', "/");
            if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'[') {
                return crate::json::specs::parse(&bytes)
                    .map_err(invalid)?
                    .into_iter()
                    .map(|case| {
                        let metadata = CaseMetadata {
                            id: format!("json:{name}:{}", case.name),
                            source: Source::Json,
                            group: "excel_specs".into(),
                            suite: name.clone(),
                            name: case.name.clone(),
                            location: case
                                .steps
                                .first()
                                .and_then(|s| s.assertions.first())
                                .map(|a| format!("{}!{}", a.target.sheet, a.target.address))
                                .unwrap_or_default(),
                            formula: Some(
                                case.cells
                                    .iter()
                                    .filter_map(|c| match &c.content {
                                        crate::json::specs::CellContent::Formula(f) => {
                                            Some(f.as_str())
                                        }
                                        _ => None,
                                    })
                                    .collect::<Vec<_>>()
                                    .join("; "),
                            ),
                            authority: case.compatibility.clone(),
                            evidence: case
                                .source_urls
                                .iter()
                                .cloned()
                                .chain(std::iter::once(case.rationale.clone()))
                                .collect(),
                            input_digest: input_digest.clone(),
                            fods: None,
                        };
                        Ok(Box::new(ExcelSpecSuite {
                            cases: vec![metadata],
                            case,
                        }) as Box<dyn TestSuite>)
                    })
                    .collect();
            }
            let suite = crate::json::parse_suite(std::str::from_utf8(&bytes)?)
                .map_err(|error| invalid(format!("{}: {error}", relative.display())))?;
            let name = relative.to_string_lossy().replace('\\', "/");
            let group = suite
                .group
                .clone()
                .or_else(|| groups.get(&name).cloned())
                .unwrap_or_else(|| name.trim_end_matches(".json").to_owned());
            let cases = suite
                .scenarios
                .iter()
                .map(|scenario| CaseMetadata {
                    id: format!("json:{name}:{}", scenario.name),
                    source: Source::Json,
                    group: group.clone(),
                    suite: name.clone(),
                    name: scenario.name.clone(),
                    location: scenario
                        .actions
                        .iter()
                        .find_map(|action| match action {
                            crate::json::schema::Action::Check { cell, .. } => Some(cell.clone()),
                            _ => None,
                        })
                        .unwrap_or_default(),
                    formula: Some(
                        suite
                            .workbook
                            .cells
                            .iter()
                            .filter_map(|cell| match &cell.content {
                                crate::json::schema::Content::Formula(formula) => {
                                    Some(formula.formula.as_str())
                                }
                                _ => None,
                            })
                            .chain(scenario.actions.iter().filter_map(|action| match action {
                                crate::json::schema::Action::Set {
                                    content: crate::json::schema::Content::Formula(formula),
                                    ..
                                } => Some(formula.formula.as_str()),
                                _ => None,
                            }))
                            .collect::<Vec<_>>()
                            .join("; "),
                    ),
                    authority: format!(
                        "Authored expectations; {:?}; consult evidence",
                        suite.builtin_policy
                    ),
                    evidence: scenario.evidence.clone(),
                    input_digest: input_digest.clone(),
                    fods: None,
                })
                .collect();
            Ok(vec![
                Box::new(JsonSuite { cases, suite }) as Box<dyn TestSuite>
            ])
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|suites| suites.into_iter().flatten().collect())
}

// Fingerprints are persisted observations, independent of serde_json feature
// unification (GPUI enables preserve_order). Keep the original sorted-key bytes.
fn json_observation_fingerprint(mut value: serde_json::Value) -> Result<String, Error> {
    value.sort_all_objects();
    Ok(model::digest(&serde_json::to_vec(&value)?))
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn json_fingerprints_sort_nested_objects_without_changing_array_or_scalar_identity() {
        let inserted = serde_json::from_str(r#"{"z":false,"a":[{"z":2,"a":1},"1"]}"#).unwrap();
        let canonical = br#"{"a":[{"a":1,"z":2},"1"],"z":false}"#;
        let expected = model::digest(canonical);
        assert_eq!(json_observation_fingerprint(inserted).unwrap(), expected);
        for changed in [
            r#"{"a":["1",{"a":1,"z":2}],"z":false}"#,
            r#"{"a":[{"a":1,"z":2},1],"z":false}"#,
            r#"{"a":[{"a":1,"z":2},"1"],"z":true}"#,
        ] {
            assert_ne!(
                json_observation_fingerprint(serde_json::from_str(changed).unwrap()).unwrap(),
                expected
            );
        }
    }
}
