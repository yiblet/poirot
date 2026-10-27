//! Explicit skips never masquerade as known executed failures.
use crate::{
    RunnerError,
    model::{CaseMetadata, Source},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

/// An explicit exemption from executing one exact stable case ID.
/// A nonempty reason and optional tracking reference make skips visible separately
/// from executed known failures.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkipRule {
    pub case: String,
    pub reason: String,
    pub tracking: Option<String>,
}

/// Loaded skip rules matched against the discovered case inventory.
/// Loading rejects duplicate, unexplained, or stale rules for the selected sources.
#[derive(Default, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skips {
    pub rules: Vec<SkipRule>,
}

impl Skips {
    pub fn load(
        path: Option<&Path>,
        inventory: &[CaseMetadata],
        sources: &[Source],
    ) -> Result<Self, RunnerError> {
        let rules: Self = match path {
            None => Self::default(),
            Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        };
        let ids: BTreeSet<_> = inventory.iter().map(|case| case.id.as_str()).collect();
        let mut seen = BTreeSet::new();
        for rule in &rules.rules {
            if rule.reason.trim().is_empty() || !seen.insert(&rule.case) {
                return Err(RunnerError::InvalidSkipRule);
            }
            if !ids.contains(rule.case.as_str()) {
                // Source selection avoids opening unrelated corpora. Their rules are
                // validated when those sources are discovered, not silently applied here.
                if !sources.is_empty()
                    && [Source::Fods, Source::Json].iter().any(|source| {
                        !sources.contains(source) && rule.case.starts_with(&format!("{source}:"))
                    })
                {
                    continue;
                }
                return Err(RunnerError::UnknownSkipCase {
                    case: rule.case.clone(),
                });
            }
        }
        Ok(rules)
    }

    pub fn reason(&self, id: &str) -> Option<String> {
        self.rules
            .iter()
            .find(|rule| rule.case == id)
            .map(|rule| match &rule.tracking {
                Some(tracking) => format!("{} ({tracking})", rule.reason),
                None => rule.reason.clone(),
            })
    }
}
