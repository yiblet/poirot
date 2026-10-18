use poirot::{CellValue, ExcelText};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suite {
    pub schema_version: u8,
    /// Optional display family; the suite filename is the default.
    #[serde(default)]
    pub group: Option<String>,
    /// Explicit suite contract; omitted metadata preserves pinned legacy behavior.
    #[serde(default)]
    pub builtin_policy: poirot::eval::BuiltinSemanticsPolicy,
    pub workbook: WorkbookDefinition,
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkbookDefinition {
    pub sheets: Vec<String>,
    #[serde(default)]
    pub cells: Vec<InitialCell>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCell {
    pub cell: String,
    pub content: Content,
    pub number_format: Option<String>,
}

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Formula {
    pub formula: String,
}

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
