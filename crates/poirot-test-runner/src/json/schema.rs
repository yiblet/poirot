use poirot::CellValue;
use poirot::cells::ExcelText;
use serde::{Deserialize, Serialize};

/// Authored JSON test suite with a common starting workbook and explicit evaluation policies.
/// Each scenario starts independently from that workbook and runs through both
/// normal and incremental-overlay evaluation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suite {
    pub schema_version: u8,
    /// Optional display family; the suite filename is the default.
    #[serde(default)]
    pub group: Option<String>,
    /// Explicit suite builtin contract; omitted metadata uses modern Excel defaults.
    #[serde(default)]
    pub builtin_policy: poirot::eval::BuiltinSemanticsPolicy,
    /// Array publication contract independently established by the fixture evidence.
    #[serde(default)]
    pub array_result_policy: poirot::eval::ArrayResultPolicy,
    pub workbook: WorkbookDefinition,
    pub scenarios: Vec<Scenario>,
}

/// Initial worksheet names and sparse authored cells used to construct a fresh
/// workbook for each scenario execution.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbookDefinition {
    pub sheets: Vec<String>,
    #[serde(default)]
    pub cells: Vec<InitialCell>,
}

/// One cell written into the scenario's starting workbook before actions execute.
/// Content is a typed literal or an explicitly wrapped formula, with an optional display format.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCell {
    pub cell: String,
    pub content: Content,
    pub number_format: Option<String>,
}

/// An ordered edit-and-check sequence run from a fresh copy of the suite workbook.
/// Its evidence records why the authored expectations are valid independently of Poirot output.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    /// Reviewed evidence for expected behavior, never generated from Poirot output.
    pub evidence: Vec<String>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    Set {
        cell: String,
        content: Content,
    },
    Format {
        cell: String,
        number_format: Option<String>,
    },
    Check {
        cell: String,
        expected: Scalar,
        /// Absolute numeric tolerance; zero (the default) means exact equality.
        #[serde(default)]
        tolerance: f64,
        display: Option<String>,
        number_format: Option<String>,
        known_gap: Option<KnownGap>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Formula(Formula),
    Value(Scalar),
}

/// Explicit formula wrapper that distinguishes formula source from an ordinary text literal
/// when decoding authored cell content.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Formula {
    pub formula: String,
}

/// Reviewed annotation accepting one exact incorrect result as a known compatibility gap.
/// A different result still fails, and reaching the authored expectation marks the annotation stale.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownGap {
    pub reason: String,
    /// Only this exact observed value is accepted as the documented gap.
    pub actual: Scalar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Scalar {
    Blank,
    Number(f64),
    Text(String),
    Utf16(Vec<u16>),
    Boolean(bool),
    Error(String),
}

impl Scalar {
    pub fn cell_value(&self) -> Result<CellValue, String> {
        Ok(match self {
            Self::Blank => CellValue::Blank,
            Self::Number(n) if n.is_finite() => CellValue::Number(*n),
            Self::Number(_) => return Err("numbers must be finite".into()),
            Self::Text(s) => CellValue::Text(s.clone()),
            Self::Utf16(units) => ExcelText::from_units(units.clone()).into(),
            Self::Boolean(b) => CellValue::Boolean(*b),
            Self::Error(s) => CellValue::Error(s.parse().map_err(|e| format!("{e}"))?),
        })
    }

    pub fn from_cell(value: &CellValue) -> Self {
        match value {
            CellValue::Blank => Self::Blank,
            CellValue::Number(n) => Self::Number(*n),
            CellValue::Text(s) => Self::Text(s.clone()),
            CellValue::TextUtf16(s) => Self::Utf16(s.units().to_vec()),
            CellValue::Boolean(b) => Self::Boolean(*b),
            CellValue::Error(e) => Self::Error(e.to_string()),
        }
    }
}
