//! Typed corpus metadata and integrity-checked native fixture loading.
use crate::fods::{
    FodsError,
    xml::{
        SOURCE_COMMIT,
        model::{FodsWorkbook, Omission},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Return the repository-owned corpus root for this runner.
pub fn default_corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/fods")
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
    pub sheet: Option<String>,
    /// Source A1 coordinate.
    pub cell: Option<String>,
    /// Recorded pass or known-gap expectation.
    pub classification: BaselineStatus,
    /// Optional explanation for the current baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Optional structured diagnostic category for a known gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_type: Option<crate::fods::gaps::GapKind>,
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

fn read_metadata<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<T, FodsError> {
    let bytes = std::fs::read(&path).map_err(|source| FodsError::Io {
        path: path.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| FodsError::Metadata { path, source })
}

fn relative_path(name: &str) -> bool {
    !name.is_empty()
        && Path::new(name)
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

impl Corpus {
    /// Load metadata and reject invalid paths, duplicate identities, or inconsistent provenance.
    pub fn open(root: &Path) -> Result<Self, FodsError> {
        let manifest: CorpusManifest = read_metadata(root.join("manifest.json"))?;
        let baseline: Baseline = if root.join("classifications.json").exists() {
            // Standalone imported corpora retain their native metadata format.
            read_metadata(root.join("classifications.json"))?
        } else {
            let shared =
                crate::baseline::Baseline::load(&crate::package_root().join("baseline.json"))
                    .map_err(|source| FodsError::CombinedBaseline {
                        source: Box::new(source),
                    })?;
            if shared.schema_version == 2 {
                let mut cases = Vec::new();
                for entry in &manifest.fixtures {
                    for index in 0..entry.cases {
                        let id = format!("{}__{index:05}", entry.id);
                        let previous = shared.cases.get(&format!("fods:{id}"));
                        let observation = previous.map(|entry| &entry.observation);
                        cases.push(BaselineCase {
                            id,
                            fixture: entry.fixture.clone(),
                            index,
                            sheet: None,
                            cell: None,
                            classification: if observation
                                .is_none_or(|o| o.outcome == crate::model::Outcome::Passed)
                            {
                                BaselineStatus::Pass
                            } else {
                                BaselineStatus::Gap
                            },
                            reason: previous.and_then(|entry| entry.reason.clone()),
                            gap_type: observation.and_then(|o| o.category.clone()).and_then(
                                |category| crate::fods::gaps::GapKind::from_label(&category),
                            ),
                        });
                    }
                }
                Baseline {
                    source_commit: shared.fods_source_commit,
                    cases,
                }
            } else {
                let mut cases: Vec<_> = shared
                    .cases
                    .into_iter()
                    .filter_map(|(id, entry)| {
                        let binding = entry.fods?;
                        // Removed fixtures/case slots remain in the shared comparison history,
                        // but are not execution bindings for the current manifest.
                        if !manifest.fixtures.iter().any(|fixture| {
                            fixture.fixture == entry.suite && binding.index < fixture.cases
                        }) {
                            return None;
                        }
                        let id = id.strip_prefix("fods:")?.to_owned();
                        let classification =
                            if entry.observation.outcome == crate::model::Outcome::Passed {
                                BaselineStatus::Pass
                            } else {
                                BaselineStatus::Gap
                            };
                        let gap_type = entry
                            .observation
                            .category
                            .and_then(|category| crate::fods::gaps::GapKind::from_label(&category));
                        Some(BaselineCase {
                            id,
                            fixture: entry.suite,
                            index: binding.index,
                            sheet: Some(binding.sheet),
                            cell: Some(binding.cell),
                            classification,
                            reason: entry.reason,
                            gap_type,
                        })
                    })
                    .collect();
                let mut bound: std::collections::HashSet<_> =
                    cases.iter().map(|case| case.id.clone()).collect();
                for entry in &manifest.fixtures {
                    if (0..entry.cases)
                        .all(|index| bound.contains(&format!("{}__{index:05}", entry.id)))
                    {
                        continue;
                    }
                    for (index, case) in load_fixture(root, entry)?.cases.into_iter().enumerate() {
                        if bound.insert(case.id.clone()) {
                            // This is only a native execution binding. The shared comparator
                            // still reports the absent historical observation as NewTest.
                            cases.push(BaselineCase {
                                id: case.id,
                                fixture: entry.fixture.clone(),
                                index,
                                sheet: Some(case.sheet_name),
                                cell: Some(case.cell),
                                classification: BaselineStatus::Pass,
                                reason: None,
                                gap_type: None,
                            });
                        }
                    }
                }
                Baseline {
                    source_commit: shared.fods_source_commit,
                    cases,
                }
            }
        };
        if root.join("classifications.json").exists()
            && baseline
                .cases
                .iter()
                .any(|case| case.sheet.is_none() || case.cell.is_none())
        {
            return Err(FodsError::MissingNativeBindings);
        }
        if manifest.source_commit != baseline.source_commit
            || manifest.source_commit != SOURCE_COMMIT
        {
            return Err(FodsError::RevisionMismatch);
        }
        let mut fixtures = HashMap::new();
        let mut count = 0usize;
        for entry in &manifest.fixtures {
            if !relative_path(&entry.fixture) || !relative_path(&entry.source) {
                return Err(FodsError::InvalidFixturePath {
                    fixture: entry.fixture.clone(),
                });
            }
            if entry.sha256.len() != 64 || !entry.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(FodsError::InvalidDigest {
                    fixture: entry.fixture.clone(),
                });
            }
            if fixtures.insert(entry.fixture.clone(), entry).is_some() {
                return Err(FodsError::DuplicateFixture {
                    fixture: entry.fixture.clone(),
                });
            }
            count = count
                .checked_add(entry.cases)
                .ok_or(FodsError::CaseCountOverflow)?;
        }
        if count != manifest.case_count {
            return Err(FodsError::ManifestCountMismatch);
        }
        let mut records = HashMap::new();
        let mut slots = std::collections::HashSet::new();
        for case in baseline.cases {
            let entry = fixtures
                .get(&case.fixture)
                .ok_or_else(|| FodsError::UnknownFixture {
                    fixture: case.fixture.clone(),
                })?;
            if case.index >= entry.cases || case.id != format!("{}__{:05}", entry.id, case.index) {
                return Err(FodsError::InvalidBaselineBinding {
                    case: case.id.clone(),
                });
            }
            if !slots.insert((case.fixture.clone(), case.index))
                || records.insert(case.id.clone(), case).is_some()
            {
                return Err(FodsError::DuplicateBaselineBinding);
            }
        }
        if records.len() != manifest.case_count {
            return Err(FodsError::IncompleteBaseline);
        }
        Ok(Self {
            root: root.to_owned(),
            manifest,
            baseline: records,
        })
    }
}

/// Read and hash-check a native fixture before extracting its workbook/cases.
pub fn load_fixture(root: &Path, entry: &FixtureEntry) -> Result<FodsWorkbook, FodsError> {
    if !relative_path(&entry.fixture) {
        return Err(FodsError::InvalidFixturePath {
            fixture: entry.fixture.clone(),
        });
    }
    let path = root.join("fixtures").join(&entry.fixture);
    let bytes = std::fs::read(&path).map_err(|source| FodsError::Io {
        path: path.clone(),
        source,
    })?;
    if crate::model::digest(&bytes) != entry.sha256 {
        return Err(FodsError::DigestMismatch {
            fixture: entry.fixture.clone(),
            expected: entry.sha256.clone(),
            actual: crate::model::digest(&bytes),
        });
    }
    let data = crate::fods::xml::extract_bytes(&bytes, &entry.source)
        .map_err(|source| FodsError::Fods { path, source })?;
    if data.cases.len() != entry.cases {
        return Err(FodsError::CaseCountMismatch {
            fixture: entry.fixture.clone(),
            expected: entry.cases,
            actual: data.cases.len(),
        });
    }
    Ok(data)
}
