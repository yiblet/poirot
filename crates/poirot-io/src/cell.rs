//! Authored cell contents, presentation, and shared formula provenance.
use crate::value_wire::value_serde;
use crate::{CellAddress, CellValue, ModelError, VisualStyle};

/// Cell presentation metadata.
/// The [`Cell`] example applies a number format to a formula and a stored blank.
///
/// A number format changes display, not the stored value. Absence selects the
/// consumer's default format. Styles can belong to explicitly stored blank cells;
/// they do not require a literal input or a calculated result.
/// See [`VisualStyle`] for applying shared font presentation and a number format to a cell.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct CellStyle {
    /// Optional Excel format spelling; affects display rather than stored value.
    pub number_format: Option<String>,
    /// Optional shared visual presentation.
    pub visual: Option<std::sync::Arc<VisualStyle>>,
}
impl CellStyle {
    /// Count logical format and font-name bytes, including shared visual payloads per cell.
    pub fn text_bytes(&self) -> Option<usize> {
        self.number_format
            .as_ref()
            .map_or(0, String::len)
            .checked_add(self.visual.as_ref().map_or(0, |style| style.text_bytes()))
    }
}

/// Worksheet-local identity of an imported XLSX shared-formula group.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct SharedFormulaId(
    /// Source group number, unique only within one worksheet.
    pub u32,
);

/// Advisory shared-formula provenance; complete cell formula text remains authoritative.
///
/// The origin need not remain a live formula cell. This is unrelated to spill ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SharedFormula {
    /// Shared group identity, scoped to the containing worksheet.
    pub group: SharedFormulaId,
    /// Original template coordinate used when the group was imported.
    pub origin: CellAddress,
}
impl SharedFormula {
    /// Report whether a member occupies the original template coordinate.
    pub fn is_anchor(self, address: CellAddress) -> bool {
        self.origin == address
    }
}

/// Authored worksheet content and its saved result, independent of live calculation.
///
/// For formula cells, `value` is the saved result and can be stale until recalculation.
/// For literal cells, it is the input value. An explicit blank is still a stored
/// cell and may carry style; an absent coordinate has no `Cell` at all.
/// Evaluator results remain separate until explicitly written back to the workbook.
///
/// ```rust
/// use poirot_io::{Cell, CellValue};
///
/// // The cache is supplied by an importer or application, not calculated here.
/// let mut cell = Cell::with_formula("=B2*0.2", CellValue::Number(25.))?;
/// cell.style.number_format = Some("0.00".into());
/// assert_eq!(cell.formula.as_deref(), Some("B2*0.2"));
/// assert_eq!(cell.value, CellValue::Number(25.));
/// let styled_blank = Cell { style: cell.style.clone(), ..Cell::default() };
/// assert_eq!(styled_blank.value, CellValue::Blank);
/// assert_eq!(styled_blank.style.number_format.as_deref(), Some("0.00"));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    /// Literal input or potentially stale saved formula result.
    #[serde(with = "value_serde")]
    pub value: CellValue,
    /// Normalized formula source; None denotes a literal cell.
    pub formula: Option<String>,
    /// Optional XLSX group provenance, preserved by cache/style edits.
    ///
    /// Source setters detach membership. Direct field edits can leave this stale;
    /// serializers must verify the complete formula before emitting shared records.
    pub shared_formula: Option<SharedFormula>,
    /// Presentation metadata retained independently of calculation.
    pub style: CellStyle,
}
impl Cell {
    /// Create an unstyled literal cell.
    pub fn new(value: impl Into<CellValue>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }
    /// Create a formula cell with a caller-supplied stored result.
    ///
    /// Trims whitespace and an optional leading `=`. An empty formula is rejected;
    /// the stored result is not calculated or checked against the formula.
    pub fn with_formula(formula: impl Into<String>, cached: CellValue) -> Result<Self, ModelError> {
        Ok(Self {
            value: cached,
            formula: Some(normalize_formula(formula.into())?),
            shared_formula: None,
            style: CellStyle::default(),
        })
    }
    /// Check provenance bounds and require an accompanying formula source.
    pub fn validate_shared_formula(&self) -> Result<(), ModelError> {
        if let Some(shared) = self.shared_formula {
            shared.origin.validate()?;
            if self.formula.is_none() {
                return Err(ModelError("shared formula requires formula source".into()));
            }
        }
        Ok(())
    }
}
/// Normalize authored formula text and reject empty expressions.
pub fn normalize_formula(formula: String) -> Result<String, ModelError> {
    let text = formula
        .trim()
        .strip_prefix('=')
        .unwrap_or(formula.trim())
        .trim();
    if text.is_empty() {
        Err(ModelError("formula must not be empty".into()))
    } else if text.len() == formula.len() {
        Ok(formula)
    } else {
        Ok(text.into())
    }
}
