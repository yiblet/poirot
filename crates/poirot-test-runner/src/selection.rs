//! One selector is used by discovery, execution and saved-report inspection.
use crate::model::{CaseMetadata, Source};
use clap::Args;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Args, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    /// Source adapters to include (repeat to select both).
    #[arg(long, value_enum)]
    pub source: Vec<Source>,
    /// Family or suite path component (case insensitive; repeated groups are OR).
    #[arg(long)]
    pub group: Vec<String>,
    /// Search IDs, suite paths, names and formulas (case insensitive; repeated filters are OR).
    #[arg(long)]
    pub filter: Vec<String>,
    /// Exact stable case ID (repeat for multiple cases).
    #[arg(long = "case")]
    pub cases: Vec<String>,
}

impl Selection {
    pub fn matches(&self, case: &CaseMetadata) -> bool {
        (self.source.is_empty() || self.source.contains(&case.source))
            && (self.group.is_empty()
                || self.group.iter().any(|g| {
                    case.group.eq_ignore_ascii_case(g)
                        || case
                            .suite
                            .split(['/', '-', '.'])
                            .any(|part| part.eq_ignore_ascii_case(g))
                }))
            && (self.cases.is_empty() || self.cases.contains(&case.id))
            && (self.filter.is_empty() || {
                let haystack = format!(
                    "{} {} {} {} {}",
                    case.id,
                    case.suite,
                    case.name,
                    case.location,
                    case.formula.as_deref().unwrap_or_default()
                )
                .to_lowercase();
                self.filter
                    .iter()
                    .any(|f| haystack.contains(&f.to_lowercase()))
            })
    }

    pub fn unrestricted(&self) -> bool {
        self.source.is_empty()
            && self.group.is_empty()
            && self.filter.is_empty()
            && self.cases.is_empty()
    }
}
