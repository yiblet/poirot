//! Execution observations and historical comparisons are independent dimensions.
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Fods,
    Json,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Fods => "fods",
            Self::Json => "json",
        })
    }
}

/// Retained native identity needed to validate the FODS manifest binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FodsBinding {
    pub index: usize,
    pub sheet: String,
    pub cell: String,
}

/// Source-independent identity and evidence for a discovered test case.
/// Selection, reports, and baseline binding use this metadata without needing to
/// interpret the fixture's native execution format.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseMetadata {
    pub id: String,
    pub source: Source,
    pub group: String,
    pub suite: String,
    pub name: String,
    pub location: String,
    pub formula: Option<String>,
    pub authority: String,
    pub evidence: Vec<String>,
    pub input_digest: String,
    pub fods: Option<FodsBinding>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Passed,
    Mismatch,
    Blocked,
    Skipped,
    HarnessError,
}

/// Comparable facts from one execution, independent of its historical baseline status.
/// A category and stable diagnostic fingerprint distinguish changed failures even
/// when their broad outcome stays the same.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Stable digest of diagnostic facts, excluding status and presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl Observation {
    pub fn passed() -> Self {
        Self {
            outcome: Outcome::Passed,
            category: None,
            fingerprint: None,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    #[default]
    Unchanged,
    Regression,
    Improvement,
    NewTest,
    ChangedGap,
    NewSkip,
    ChangedSkip,
}

/// One executed or explicitly skipped case joined to its prior observation and change classification.
/// Native result details remain available for inspection while the shared observation
/// drives baseline comparison and run success.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaseResult {
    pub case: CaseMetadata,
    pub observation: Observation,
    pub reason: String,
    /// Native typed results preserve both modes and raw ordered change lists.
    pub details: serde_json::Value,
    #[serde(default)]
    pub change: Change,
    pub baseline: Option<Observation>,
}

impl CaseResult {
    pub fn fails(&self, strict: bool) -> bool {
        self.observation.outcome == Outcome::HarnessError
            || self.observation.category.as_deref() == Some("stale_annotation")
            || matches!(
                self.change,
                Change::Regression | Change::ChangedGap | Change::NewSkip | Change::ChangedSkip
            )
            || (self.change == Change::NewTest && self.observation.outcome != Outcome::Passed)
            || (strict && self.observation.outcome != Outcome::Passed)
    }
}

/// Compute the lowercase SHA-256 fingerprint used for corpus identity.
pub fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::digest;

    #[test]
    fn digest_preserves_lowercase_hex_and_leading_zeroes() {
        // Expected fingerprints independently verified with Python's hashlib.sha256.
        assert_eq!(
            digest(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
