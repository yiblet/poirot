//! Checked-in observations; authored expected values are never rewritten here.
use crate::{
    Error, invalid,
    model::{CaseResult, Change, FodsBinding, Observation, Outcome, Source},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub source: Source,
    pub group: String,
    pub suite: String,
    pub observation: Observation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fods: Option<FodsBinding>,
}

#[derive(Clone, Debug)]
pub struct Baseline {
    pub schema_version: u8,
    pub fods_source_commit: String,
    pub cases: BTreeMap<String, Entry>,
}

// Schema 2 stores passing IDs only and groups identical nonpassing observations.
// Source bindings are reconstructed from the hash-checked corpus, not repeated here.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SparseGroup {
    observation: Observation,
    reason: String,
    cases: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredBaseline {
    schema_version: u8,
    fods_source_commit: String,
    #[serde(default)]
    passing: Vec<String>,
    #[serde(default)]
    nonpassing: Vec<SparseGroup>,
}

impl Serialize for Baseline {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Baseline", 4)?;
        state.serialize_field("schema_version", &self.schema_version)?;
        state.serialize_field("fods_source_commit", &self.fods_source_commit)?;
        {
            let mut passing = Vec::new();
            let mut nonpassing: BTreeMap<String, SparseGroup> = BTreeMap::new();
            for (id, entry) in &self.cases {
                if entry.observation.outcome == Outcome::Passed {
                    passing.push(id);
                } else {
                    let reason = entry.reason.clone().unwrap_or_default();
                    let key = serde_json::to_string(&(&entry.observation, &reason))
                        .map_err(serde::ser::Error::custom)?;
                    nonpassing
                        .entry(key)
                        .or_insert_with(|| SparseGroup {
                            observation: entry.observation.clone(),
                            reason,
                            cases: Vec::new(),
                        })
                        .cases
                        .push(id.clone());
                }
            }
            state.serialize_field("passing", &passing)?;
            state.serialize_field("nonpassing", &nonpassing.into_values().collect::<Vec<_>>())?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for Baseline {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let stored = StoredBaseline::deserialize(deserializer)?;
        let mut cases = BTreeMap::new();
        match stored.schema_version {
            2 => {
                let mut insert = |id: String, observation: Observation, reason: Option<String>| {
                    let source = if id.starts_with("fods:") {
                        Source::Fods
                    } else if id.starts_with("json:") {
                        Source::Json
                    } else {
                        return Err(D::Error::custom("invalid baseline case ID"));
                    };
                    let entry = Entry {
                        source,
                        group: String::new(),
                        suite: String::new(),
                        observation,
                        reason,
                        fods: None,
                    };
                    if cases.insert(id.clone(), entry).is_some() {
                        return Err(D::Error::custom(format!(
                            "duplicate baseline case ID: {id}"
                        )));
                    }
                    Ok(())
                };
                for id in stored.passing {
                    insert(id, Observation::passed(), None)?;
                }
                for group in stored.nonpassing {
                    if group.observation.outcome == Outcome::Passed || group.cases.is_empty() {
                        return Err(D::Error::custom(
                            "nonpassing groups must contain nonpassing cases",
                        ));
                    }
                    for id in group.cases {
                        insert(id, group.observation.clone(), Some(group.reason.clone()))?;
                    }
                }
            }
            _ => return Err(D::Error::custom("unsupported or mixed baseline schema")),
        }
        Ok(Self {
            schema_version: stored.schema_version,
            fods_source_commit: stored.fods_source_commit,
            cases,
        })
    }
}

impl Baseline {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let baseline: Self = serde_json::from_slice(&std::fs::read(path)?)?;
        if baseline.schema_version != 2
            || baseline.fods_source_commit != crate::fods::xml::SOURCE_COMMIT
        {
            return Err(invalid(
                "baseline must use schema 2 and the pinned FODS source revision",
            ));
        }
        for (id, entry) in &baseline.cases {
            if id.is_empty()
                || !id.starts_with(&format!("{}:", entry.source))
                || matches!(entry.observation.outcome, Outcome::HarnessError)
                || (entry.observation.outcome == Outcome::Passed
                    && (entry.observation.category.is_some()
                        || entry.observation.fingerprint.is_some()))
                || (entry.observation.outcome != Outcome::Passed
                    && entry.reason.as_ref().is_none_or(|r| r.trim().is_empty()))
            {
                return Err(invalid(format!("invalid baseline entry: {id}")));
            }
        }
        Ok(baseline)
    }

    pub fn compare(&self, result: &mut CaseResult) {
        let prior = self
            .cases
            .get(&result.case.id)
            .map(|entry| &entry.observation);
        result.change = compare(prior, &result.observation);
        result.baseline = prior.cloned();
    }
}

pub fn compare(previous: Option<&Observation>, current: &Observation) -> Change {
    let Some(previous) = previous else {
        return Change::NewTest;
    };
    if current == previous {
        return Change::Unchanged;
    }
    if current.outcome == Outcome::Skipped {
        return if previous.outcome == Outcome::Skipped {
            Change::ChangedSkip
        } else {
            Change::NewSkip
        };
    }
    // A skipped test resuming execution must be assessed afresh, not called an improvement.
    if previous.outcome == Outcome::Skipped {
        return Change::NewTest;
    }
    if current.outcome == Outcome::HarnessError {
        return Change::Regression;
    }
    if current.outcome == Outcome::Passed {
        return Change::Improvement;
    }
    if previous.outcome == Outcome::Passed {
        return Change::Regression;
    }
    Change::ChangedGap
}

/// Produce an explicit candidate from a complete report. Partial runs cannot erase history.
pub fn candidate(report: &crate::runner::RunReport, reason: &str) -> Result<Baseline, Error> {
    if !report.header.complete_inventory
        || !report.header.selection.unrestricted()
        || report.summary.selected == 0
        || report.cases.len() != report.header.inventory.len()
        || !report
            .removed
            .iter()
            .all(|id| !report.header.inventory.iter().any(|case| &case.id == id))
    {
        return Err(invalid(
            "baseline updates require a completed, unfiltered run of both default corpora",
        ));
    }
    if reason.trim().is_empty() {
        return Err(invalid("baseline updates need a review reason"));
    }
    let mut cases = BTreeMap::new();
    for result in &report.cases {
        if result.observation.outcome == Outcome::HarnessError
            || result.observation.category.as_deref() == Some("stale_annotation")
        {
            return Err(invalid(
                "harness errors and stale JSON annotations cannot be accepted into a baseline",
            ));
        }
        if cases
            .insert(
                result.case.id.clone(),
                Entry {
                    source: result.case.source,
                    group: result.case.group.clone(),
                    suite: result.case.suite.clone(),
                    observation: result.observation.clone(),
                    reason: (result.observation.outcome != Outcome::Passed)
                        .then(|| format!("{reason}: {}", result.reason)),
                    fods: result.case.fods.clone(),
                },
            )
            .is_some()
        {
            return Err(invalid("duplicate case in report"));
        }
    }
    Ok(Baseline {
        schema_version: 2,
        fods_source_commit: report.header.fods_source_commit.clone(),
        cases,
    })
}

/// Publish only after serialization succeeds; refuse to overwrite an existing candidate.
pub fn write_candidate(path: &Path, value: &Baseline) -> Result<(), Error> {
    use std::io::Write;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = std::fs::remove_file(path);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_baseline_preserves_observations_and_inventory_without_passing_details() {
        let observation = Observation {
            outcome: Outcome::Blocked,
            category: Some("missing_function".into()),
            fingerprint: Some("same".into()),
        };
        let entry = |observation, reason| Entry {
            source: Source::Json,
            group: "text".into(),
            suite: "suite.json".into(),
            observation,
            reason,
            fods: None,
        };
        let baseline = Baseline {
            schema_version: 2,
            fods_source_commit: crate::fods::xml::SOURCE_COMMIT.into(),
            cases: BTreeMap::from([
                (
                    "json:suite.json:passing".into(),
                    entry(Observation::passed(), None),
                ),
                (
                    "json:suite.json:first gap".into(),
                    entry(observation.clone(), Some("known gap".into())),
                ),
                (
                    "json:suite.json:second gap".into(),
                    entry(observation, Some("known gap".into())),
                ),
            ]),
        };
        let bytes = serde_json::to_vec(&baseline).unwrap();
        let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(wire.get("cases").is_none());
        assert_eq!(
            wire["passing"],
            serde_json::json!(["json:suite.json:passing"])
        );
        assert_eq!(wire["nonpassing"].as_array().unwrap().len(), 1);
        let restored: Baseline = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            baseline.cases.keys().collect::<Vec<_>>(),
            restored.cases.keys().collect::<Vec<_>>()
        );
        for (id, entry) in &baseline.cases {
            assert_eq!(restored.cases[id].observation, entry.observation);
            assert_eq!(restored.cases[id].reason, entry.reason);
        }
        assert_eq!(
            compare(
                Some(&restored.cases["json:suite.json:passing"].observation),
                &restored.cases["json:suite.json:first gap"].observation
            ),
            Change::Regression
        );
        assert_eq!(
            compare(
                Some(&restored.cases["json:suite.json:first gap"].observation),
                &Observation::passed()
            ),
            Change::Improvement
        );
    }

    #[test]
    fn sparse_baseline_rejects_duplicate_ids_and_passing_exception_groups() {
        for body in [
            r#""passing":["json:a:b","json:a:b"],"nonpassing":[]"#,
            r#""passing":["json:a:b"],"nonpassing":[{"observation":{"outcome":"blocked"},"reason":"known","cases":["json:a:b"]}]"#,
            r#""passing":[],"nonpassing":[{"observation":{"outcome":"passed"},"reason":"invalid","cases":["json:a:b"]}]"#,
        ] {
            let text = format!(
                r#"{{"schema_version":2,"fods_source_commit":"{}",{body}}}"#,
                crate::fods::xml::SOURCE_COMMIT
            );
            assert!(serde_json::from_str::<Baseline>(&text).is_err());
        }
    }
}
