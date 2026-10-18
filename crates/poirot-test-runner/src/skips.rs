//! Explicit skips never masquerade as known executed failures.
use crate::{
    Error, invalid,
    model::{CaseMetadata, Source},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkipRule {
    pub case: String,
    pub reason: String,
    pub tracking: Option<String>,
}

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
    ) -> Result<Self, Error> {
        let rules: Self = match path {
            None => Self::default(),
            Some(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        };
        let ids: BTreeSet<_> = inventory.iter().map(|case| case.id.as_str()).collect();
        let mut seen = BTreeSet::new();
        for rule in &rules.rules {
            if rule.reason.trim().is_empty() || !seen.insert(&rule.case) {
                return Err(invalid(
                    "skip rules require a nonempty reason and a unique exact case ID",
                ));
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
                return Err(invalid(format!(
                    "stale skip rule: unknown case {}",
                    rule.case
                )));
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
