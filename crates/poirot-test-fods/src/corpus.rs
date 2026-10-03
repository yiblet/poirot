//! Typed corpus metadata and integrity-checked native fixture loading.
use crate::{
    Error,
    fods::{
        SOURCE_COMMIT,
        model::{FodsWorkbook, Omission},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Return the repository-owned corpus root for this runner.
pub fn default_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// Manifest identity and integrity metadata for one native FODS fixture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FixtureEntry {
    /// Relative path of the native fixture inside the corpus.
    pub fixture: String,
    /// Stable fixture identity used in case keys and reports.
    pub id: String,
    /// Original pinned repository source path.
    pub source: String,
    /// Expected digest of the retained native fixture bytes.
    pub sha256: String,
    /// Expected extracted source-case count.
    pub cases: usize,
}

/// Pinned corpus provenance, fixture inventory, and known extraction omissions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorpusManifest {
    /// Upstream repository from which fixtures were extracted.
    pub source_repository: String,
    /// Exact upstream revision defining this corpus.
    pub source_commit: String,
    /// License identifier retained with source fixtures.
    pub license: String,
    /// Ordered native fixture inventory.
    pub fixtures: Vec<FixtureEntry>,
    /// Expected total extracted case count.
    pub case_count: usize,
    /// Inspectable source constructs excluded during extraction.
    #[serde(default)]
    pub omissions: Vec<Omission>,
}

/// Recorded pass/gap expectation against which new runs are compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineStatus {
    /// Case is expected to pass.
    Pass,
    /// Case is an acknowledged implementation or source-context gap.
    Gap,
}

/// Stable source-case identity and its checked baseline expectation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BaselineCase {
    /// Stable source-case key.
    pub id: String,
    /// Owning native fixture identity.
    pub fixture: String,
    /// Case position within the extracted fixture.
    pub index: usize,
    /// Source worksheet spelling.
    pub sheet: String,
    /// Source A1 coordinate.
    pub cell: String,
    /// Recorded pass or known-gap expectation.
    pub classification: BaselineStatus,
    /// Optional explanation for the current baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Optional structured diagnostic category for a known gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_type: Option<crate::gaps::GapKind>,
}

/// Pinned revision and complete recorded case expectations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Baseline {
    /// Revision required to match the corpus manifest.
    pub source_commit: String,
    /// Recorded expectations keyed by stable source identity.
    pub cases: Vec<BaselineCase>,
}

/// Validated manifest/baseline metadata and its native fixture root.
#[derive(Debug)]
pub struct Corpus {
    /// Corpus filesystem root used to load native fixtures.
    pub root: PathBuf,
    /// Validated pinned fixture inventory.
    pub manifest: CorpusManifest,
    /// Expectations indexed by stable case key.
    pub baseline: HashMap<String, BaselineCase>,
}

fn read_metadata<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<T, Error> {
    let bytes = std::fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| Error::Metadata { path, source })
}

fn relative_path(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

impl Corpus {
    /// Load metadata and reject invalid paths, duplicate identities, or inconsistent provenance.
    pub fn open(root: &Path) -> Result<Self, Error> {
        let manifest: CorpusManifest = read_metadata(root.join("manifest.json"))?;
        let baseline: Baseline = read_metadata(root.join("classifications.json"))?;
        if manifest.source_commit != baseline.source_commit
            || manifest.source_commit != SOURCE_COMMIT
        {
            return Err(Error::InvalidPlan(
                "manifest and baseline must match the pinned source revision".into(),
            ));
        }
        let mut fixtures = HashMap::new();
        let mut count = 0usize;
        for entry in &manifest.fixtures {
            if !relative_path(&entry.fixture) || !relative_path(&entry.source) {
                return Err(Error::InvalidPlan(format!(
                    "invalid fixture/source path: {}",
                    entry.fixture
                )));
            }
            if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Error::InvalidPlan(format!(
                    "invalid SHA-256 for {}",
                    entry.fixture
                )));
            }
            if fixtures.insert(entry.fixture.clone(), entry).is_some() {
                return Err(Error::InvalidPlan(format!(
                    "duplicate fixture {}",
                    entry.fixture
                )));
            }
            count = count
                .checked_add(entry.cases)
                .ok_or_else(|| Error::InvalidPlan("case count overflow".into()))?;
        }
        if count != manifest.case_count {
            return Err(Error::InvalidPlan(
                "manifest case count does not equal fixture totals".into(),
            ));
        }
        let mut records = HashMap::new();
        let mut slots = std::collections::HashSet::new();
        for case in baseline.cases {
            let entry = fixtures.get(&case.fixture).ok_or_else(|| {
                Error::InvalidPlan(format!(
                    "baseline references unknown fixture {}",
                    case.fixture
                ))
            })?;
            if case.index >= entry.cases || case.id != format!("{}__{:05}", entry.id, case.index) {
                return Err(Error::InvalidPlan(format!(
                    "invalid baseline binding {}",
                    case.id
                )));
            }
            if !slots.insert((case.fixture.clone(), case.index))
                || records.insert(case.id.clone(), case).is_some()
            {
                return Err(Error::InvalidPlan(
                    "duplicate baseline case or fixture/index binding".into(),
                ));
            }
        }
        if records.len() != manifest.case_count {
            return Err(Error::InvalidPlan(
                "baseline must bind every declared case exactly once".into(),
            ));
        }
        Ok(Self {
            root: root.to_owned(),
            manifest,
            baseline: records,
        })
    }
}

/// Read and hash-check a native fixture before extracting its workbook/cases.
pub fn load_fixture(root: &Path, entry: &FixtureEntry) -> Result<FodsWorkbook, Error> {
    if !relative_path(&entry.fixture) {
        return Err(Error::Integrity {
            fixture: entry.fixture.clone(),
            message: "invalid relative fixture path".into(),
        });
    }
    let path = root.join("fixtures").join(&entry.fixture);
    let bytes = std::fs::read(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    if format!("{:x}", Sha256::digest(&bytes)) != entry.sha256 {
        return Err(Error::Integrity {
            fixture: entry.fixture.clone(),
            message: "SHA-256 differs from pinned manifest".into(),
        });
    }
    let data = crate::fods::extract_bytes(&bytes, &entry.source)
        .map_err(|message| Error::Fods { path, message })?;
    if data.cases.len() != entry.cases {
        return Err(Error::Integrity {
            fixture: entry.fixture.clone(),
            message: "extracted case count differs from manifest".into(),
        });
    }
    Ok(data)
}
