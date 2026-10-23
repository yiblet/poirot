//! Source records used from XML parsing through execution planning.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Typed native cell input retaining ODF numeric source spellings for fidelity diagnostics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputValue {
    /// Numeric input with optional original ODF type/value metadata.
    Number {
        /// Decoded numeric input.
        value: f64,
        /// Original ODF numeric value-type spelling, if retained.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        odf_type: Option<String>,
        /// Original ODF numeric value spelling, if retained.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        odf_value: Option<String>,
    },
    /// Text input decoded from native fixture content.
    Text {
        /// Decoded source text.
        value: String,
    },
    /// Boolean native input.
    Boolean {
        /// Decoded boolean input.
        value: bool,
    },
    /// Blank source value.
    Blank,
}

impl InputValue {
    /// Construct a numeric fixture value without ODF-specific source metadata.
    pub fn number(value: f64) -> Self {
        Self::Number {
            value,
            odf_type: None,
            odf_value: None,
        }
    }
    /// Whether the fixture value denotes blank.
    pub fn is_blank(&self) -> bool {
        matches!(self, Self::Blank)
    }
    /// Borrow text only when the fixture value is textual.
    pub fn as_text(&self) -> Option<&str> {
        if let Self::Text { value } = self {
            Some(value)
        } else {
            None
        }
    }
}

/// Native array-formula footprint and this cell's position within it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatrixSpan {
    /// Declared matrix row extent.
    pub rows: u32,
    /// Declared matrix column extent.
    pub cols: u32,
    /// Zero-based matrix anchor row.
    pub origin_row: u32,
    /// Zero-based matrix anchor column.
    pub origin_col: u32,
    /// Translated formula source at the matrix anchor.
    pub formula: String,
    /// This member's row offset from the matrix anchor.
    pub offset_row: u32,
    /// This member's column offset from the matrix anchor.
    pub offset_col: u32,
}

/// Sparse extracted cell with source text, typed value, and optional matrix provenance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellInput {
    /// Zero-based source row coordinate.
    pub row: u32,
    /// Zero-based source column coordinate.
    pub col: u32,
    /// Typed native value or saved formula result.
    pub value: InputValue,
    /// Translated formula source consumed by the evaluator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    /// Original ODF formula spelling retained for diagnostics.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_formula: Option<String>,
    /// Array-formula footprint metadata when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<MatrixSpan>,
    /// Whether this source cell continues an array-formula footprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix_continuation: Option<bool>,
}

impl CellInput {
    /// Construct an explicit blank fixture cell at a coordinate.
    pub fn blank(row: u32, col: u32) -> Self {
        Self {
            row,
            col,
            value: InputValue::Blank,
            formula: None,
            original_formula: None,
            matrix: None,
            matrix_continuation: None,
        }
    }
}

/// Extracted native worksheet and its sparse source cells.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sheet {
    /// Original worksheet spelling.
    pub name: String,
    /// Sparse extracted cells in source order.
    pub cells: Vec<CellInput>,
}

/// Native defined name with translated source and its original base context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NamedDefinition {
    /// Source defined-name spelling.
    pub name: String,
    /// Translated expression backing the name.
    pub formula: String,
    /// Exact XML-decoded expression or cell-range address, before translation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_definition: Option<String>,
    /// Native base-cell spelling used to interpret relative definitions.
    pub base_cell_address: Option<String>,
}

/// Source check formula paired with the native fixture's expected/correct-result context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FormulaCase {
    /// Stable source-case identity.
    pub id: String,
    /// Zero-based worksheet index.
    pub sheet: usize,
    /// Source worksheet spelling.
    pub sheet_name: String,
    /// Zero-based check-formula row.
    pub row: u32,
    /// Zero-based check-formula column.
    pub col: u32,
    /// A1 check-formula coordinate used in reports.
    pub cell: String,
    /// Translated check expression evaluated by the runner.
    pub formula: String,
    /// Original ODF check expression.
    pub original_formula: String,
    /// Native expected-result cell used to interpret the source check.
    pub expected: CellInput,
    /// Zero-based expected-result row.
    pub correct_row: u32,
    /// Zero-based expected-result column.
    pub correct_col: u32,
    /// Translated expected-result expression.
    pub correct_formula: String,
    /// Original ODF expected-result expression.
    pub original_correct_formula: String,
    /// Array-formula context attached to the source check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<MatrixSpan>,
}

/// Reason a native source construct is excluded from the executable local model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmissionKind {
    /// Extraction encountered a construct outside its supported case model.
    #[default]
    UnsupportedConstruct,
    /// A linked external table contains cached values rather than local inputs.
    ExternalLinkedTable,
}

/// Inspectable source construct omitted from supported fixture extraction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Omission {
    /// Typed source boundary retained for selected-dependency diagnostics.
    #[serde(default)]
    pub kind: OmissionKind,
    /// Pinned fixture source path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Worksheet context, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// Cell context, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    /// Explanation of the unsupported or excluded source construct.
    pub reason: String,
}

/// Native XML attribute whose decoded source must participate in non-use proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormulaAttribute {
    /// Ordinary or matrix-anchor cell formula.
    CellFormula,
    /// Defined-name expression.
    NameExpression,
    /// Defined-name reference address.
    NameRange,
    /// Relative base address for a defined name.
    NameBase,
}

/// Original decoded source with its native attribute kind, including excluded tables.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalFormulaSource {
    /// Native source attribute kind.
    pub attribute: FormulaAttribute,
    /// Exact XML-decoded attribute value, without formula translation.
    pub source: String,
}

/// Complete extracted native fixture consumed by corpus planning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FodsWorkbook {
    /// Pinned source path identifying the native fixture.
    pub source: String,
    /// Exact source revision required for provenance matching.
    pub source_commit: String,
    /// Digest of the native fixture bytes.
    pub sha256: String,
    /// Extracted worksheets and sparse cell inputs.
    pub sheets: Vec<Sheet>,
    /// Extracted formula-backed defined names.
    pub names: Vec<NamedDefinition>,
    /// Every decoded native formula, name definition and base, including omitted tables.
    /// None means extraction did not retain a complete original-source inspection record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_formula_sources: Option<Vec<OriginalFormulaSource>>,
    /// Native calculation settings retained for context/fidelity checks.
    pub calculation_settings: Vec<BTreeMap<String, String>>,
    /// Extracted formula checks in source order.
    pub cases: Vec<FormulaCase>,
    /// Unsupported native constructs retained as inspectable omissions.
    pub omissions: Vec<Omission>,
}
