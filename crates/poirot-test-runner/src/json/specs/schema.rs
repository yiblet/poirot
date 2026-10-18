use poirot::{eval, model};
use serde::{Deserialize, Deserializer, de::Error};

/// Strictly decoded Excel specification fixture with source evidence and interleaved evaluation steps.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Case {
    /// Version of the fixture schema accepted by the runner.
    pub schema_version: u8,
    /// Stable case number linking registry coverage and fixture results.
    pub id: usize,
    /// Human-readable behavior under test.
    pub name: String,
    /// Feature group used by coverage inventory.
    pub category: String,
    /// Declared compatibility contract, distinct from observed parity.
    pub compatibility: String,
    /// Why the fixture establishes the intended behavior.
    pub rationale: String,
    /// Primary references supporting the fixture's expectations.
    pub source_urls: Vec<String>,
    /// Default text-counting contract for this case.
    pub text_policy: TextPolicy,
    /// Worksheet spellings in workbook index order.
    pub sheets: Vec<String>,
    /// Initial authored formulas and typed literal inputs.
    pub cells: Vec<Cell>,
    /// Initial workbook or sheet-scoped defined names.
    pub names: Vec<Name>,
    /// Initial workbook epoch and execution/policy settings.
    pub settings: InitialSettings,
    /// Ordered edits and assertions sharing one retained overlay.
    pub steps: Vec<Step>,
}

/// Validated worksheet coordinate decoded from an assertion or input record.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Target {
    /// Zero-based worksheet index.
    pub sheet: usize,
    /// Validated A1 coordinate converted to canonical row/column indexes.
    #[serde(deserialize_with = "cell_address")]
    pub address: model::CellAddress,
}
impl Target {
    /// Convert the fixture coordinate to the evaluator's target type.
    pub fn evaluation(self) -> eval::EvaluationTarget {
        eval::EvaluationTarget {
            sheet: self.sheet,
            address: self.address,
        }
    }
}
fn cell_address<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<model::CellAddress, D::Error> {
    let text = String::deserialize(deserializer)?;
    let address: model::CellAddress = text.parse().map_err(D::Error::custom)?;
    address.validate().map_err(D::Error::custom)?;
    Ok(address)
}

/// Exactly one authored formula or typed literal at a validated target.
#[derive(Debug, Deserialize)]
#[serde(try_from = "RawCell")]
pub(crate) struct Cell {
    /// Coordinate to write before evaluation.
    pub target: Target,
    /// Formula source or typed literal to store.
    pub content: CellContent,
}
/// Mutually exclusive formula and literal input channels.
#[derive(Debug)]
pub(crate) enum CellContent {
    /// Authored formula source.
    Formula(String),
    /// Typed literal input.
    Literal(Scalar),
}
/// Deserialization-only shape validated into exactly one formula-or-value cell.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCell {
    sheet: usize,
    #[serde(deserialize_with = "cell_address")]
    address: model::CellAddress,
    formula: Option<String>,
    value: Option<Scalar>,
}
impl TryFrom<RawCell> for Cell {
    /// Schema diagnostic when a fixture cell lacks exactly one formula-or-value channel.
    type Error = &'static str;
    fn try_from(raw: RawCell) -> Result<Self, Self::Error> {
        let content = match (raw.formula, raw.value) {
            (Some(formula), None) => CellContent::Formula(formula),
            (None, Some(value)) => CellContent::Literal(value),
            _ => return Err("cell must contain exactly one formula or typed value"),
        };
        Ok(Self {
            target: Target {
                sheet: raw.sheet,
                address: raw.address,
            },
            content,
        })
    }
}

/// Typed fixture value preserving error kinds and exact UTF-16 text.
#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Scalar {
    /// Numeric scalar.
    Number(f64),
    /// UTF-8 text scalar.
    Text(String),
    /// Boolean scalar.
    Boolean(bool),
    /// Blank scalar.
    Blank,
    /// Named spreadsheet error scalar.
    Error(ExcelError),
    /// Exact UTF-16 code units, including unpaired surrogates.
    Utf16(Vec<u16>),
}
impl Scalar {
    /// Convert fixture data into a canonical model value without evaluator execution.
    pub fn cell_value(&self) -> model::CellValue {
        match self {
            Self::Number(n) => model::CellValue::Number(*n),
            Self::Text(text) => model::CellValue::Text(text.clone()),
            Self::Boolean(b) => model::CellValue::Boolean(*b),
            Self::Blank => model::CellValue::Blank,
            Self::Error(error) => model::CellValue::Error((*error).into()),
            Self::Utf16(units) => model::ExcelText::from_units(units.clone()).into(),
        }
    }
}
/// Strict fixture spelling for supported spreadsheet error values.
#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum ExcelError {
    /// `#NULL!` reference-intersection error.
    #[serde(rename = "#NULL!")]
    Null,
    /// `#DIV/0!` zero-denominator error.
    #[serde(rename = "#DIV/0!")]
    Div0,
    /// `#VALUE!` invalid-operand error.
    #[serde(rename = "#VALUE!")]
    Value,
    /// `#REF!` invalid-reference error.
    #[serde(rename = "#REF!")]
    Ref,
    /// `#NAME?` unresolved-name error.
    #[serde(rename = "#NAME?")]
    Name,
    /// `#NUM!` numeric-domain error.
    #[serde(rename = "#NUM!")]
    Num,
    /// `#N/A` unavailable-result error.
    #[serde(rename = "#N/A")]
    Na,
    /// `#GETTING_DATA` pending external-data value.
    #[serde(rename = "#GETTING_DATA")]
    GettingData,
    /// `#SPILL!` blocked dynamic result.
    #[serde(rename = "#SPILL!")]
    Spill,
    /// `#CALC!` array-calculation error.
    #[serde(rename = "#CALC!")]
    Calc,
}
impl From<ExcelError> for model::CellError {
    fn from(error: ExcelError) -> Self {
        match error {
            ExcelError::Null => Self::Null,
            ExcelError::Div0 => Self::Div0,
            ExcelError::Value => Self::Value,
            ExcelError::Ref => Self::Ref,
            ExcelError::Name => Self::Name,
            ExcelError::Num => Self::Num,
            ExcelError::Na => Self::NA,
            ExcelError::GettingData => Self::GettingData,
            ExcelError::Spill => Self::Spill,
            ExcelError::Calc => Self::Calc,
        }
    }
}
/// Scalar or rectangular assertion values; never used to manufacture evaluator output.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum Expected {
    /// Assert one coordinate's typed value.
    Scalar(Scalar),
    /// Assert the complete rectangular footprint beginning at the target.
    Array(Array),
}
/// Explicitly tagged row-major expected footprint.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Array {
    /// Required array tag preventing ambiguous untagged decoding.
    #[serde(rename = "type")]
    pub kind: ArrayTag,
    /// Nonempty rectangular expected values in row-major order.
    pub value: Vec<Vec<Scalar>>,
}
/// Accepted tag for a rectangular expected result.
#[derive(Debug, Deserialize)]
pub(crate) enum ArrayTag {
    /// The fixture's explicit array-result tag.
    #[serde(rename = "array")]
    Array,
}

/// Formula-backed defined name installed into the fixture workbook.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Name {
    /// Defined-name spelling.
    pub name: String,
    /// Formula source backing the name.
    pub formula: String,
    /// Local sheet index, or None for workbook scope.
    pub sheet_scope: Option<usize>,
}
/// Interleavable source edits, policy updates, and output assertions on one overlay.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Step {
    /// Authored cell writes applied before this step's assertions.
    pub edits: Vec<Cell>,
    /// Outputs to request and compare after applying edits.
    pub assertions: Vec<Assertion>,
    /// Optional changes to context and overlay policies.
    pub settings: Option<SettingsUpdate>,
    /// Optional defined-name source updates.
    pub names: Option<Vec<Name>>,
    /// Whether source changes require rebuilding the prepared fixture state.
    #[serde(default)]
    pub recompile: bool,
}
/// Expected public evaluation result with tolerance, obligations, and supporting evidence.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Assertion {
    /// Output coordinate or top-left array-footprint coordinate.
    pub target: Target,
    /// Typed expectation; unresolved evidence may leave it unspecified.
    pub expected: Option<Expected>,
    /// Optional numeric comparison tolerance.
    pub tolerance: Option<f64>,
    /// Behavioral requirements this assertion is intended to exercise.
    pub obligations: Vec<String>,
    /// Source or observation provenance supporting the expectation.
    pub evidence: Evidence,
}
impl Assertion {
    /// Validate an expected footprint and expand its coordinates for public output reads.
    pub fn expected_cells(&self) -> Result<Vec<(eval::EvaluationTarget, &Scalar)>, String> {
        let origin = self.target.evaluation();
        match self
            .expected
            .as_ref()
            .ok_or("documented assertion has no expected result")?
        {
            Expected::Scalar(value) => Ok(vec![(origin, value)]),
            Expected::Array(array) => {
                let ArrayTag::Array = array.kind;
                let width = array
                    .value
                    .first()
                    .filter(|row| !row.is_empty())
                    .ok_or("empty array footprint")?
                    .len();
                if !array.value.iter().all(|row| row.len() == width) {
                    return Err("nonrectangular array footprint".into());
                }
                let mut cells = Vec::new();
                for (row, values) in array.value.iter().enumerate() {
                    for (col, value) in values.iter().enumerate() {
                        let address = model::CellAddress {
                            row: origin
                                .address
                                .row
                                .checked_add(u32::try_from(row).map_err(|e| e.to_string())?)
                                .ok_or("footprint row overflow")?,
                            col: origin
                                .address
                                .col
                                .checked_add(u32::try_from(col).map_err(|e| e.to_string())?)
                                .ok_or("footprint column overflow")?,
                        };
                        address.validate().map_err(|e| e.to_string())?;
                        cells.push((
                            eval::EvaluationTarget {
                                sheet: origin.sheet,
                                address,
                            },
                            value,
                        ));
                    }
                }
                Ok(cells)
            }
        }
    }
}

/// Strength and origin of the fixture's compatibility evidence.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvidenceKind {
    /// Expected value appears in a primary documented example.
    DocumentedExample,
    /// Expected value is derived explicitly from documented rules.
    DocumentedDerivation,
    /// Expectation is backed by a retained authentic Excel observation.
    ExcelObservation,
    /// Evidence does not yet justify a concrete parity assertion.
    Unresolved,
}
/// Auditable references and derivation/observation metadata for one expectation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Evidence {
    /// Evidence category controlling validation requirements.
    pub kind: EvidenceKind,
    /// Primary sources supporting this assertion.
    pub source_urls: Vec<String>,
    /// How the cited evidence supports the expected behavior.
    pub explanation: String,
    /// Optional review note about evidence quality or scope.
    pub review: Option<String>,
    /// Optional exact section within the cited source.
    pub source_section: Option<String>,
    /// Explicit derivation when the result is not a quoted example.
    pub derivation: Option<String>,
    /// Retained Excel execution metadata when authentic observations exist.
    pub observation: Option<Observation>,
}
impl Evidence {
    /// Render evidence context for a failed assertion diagnostic.
    pub fn describe(&self) -> String {
        let mut text = format!(
            "{:?}: {}; sources: {}",
            self.kind,
            self.explanation,
            self.source_urls.join(", ")
        );
        for detail in [&self.review, &self.source_section, &self.derivation]
            .into_iter()
            .flatten()
        {
            text.push_str(&format!("; {detail}"));
        }
        if let Some(observation) = &self.observation {
            text.push_str(&format!(
                "; Excel {} {:?} {} compatibility {} workbook {} result {}",
                observation.excel_version,
                observation.settings.date_system,
                observation.settings.locale,
                observation.settings.compatibility_version,
                observation.workbook_sha256,
                observation.result_artifact
            ));
        }
        text
    }
}
/// Retained workbook/version/settings identity for an authentic Excel result.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    /// Digest of the workbook actually run in Excel.
    pub workbook_sha256: String,
    /// Exact Excel version used for observation.
    pub excel_version: String,
    /// Workbook epoch, locale, and compatibility context used during observation.
    pub settings: ObservationSettings,
    /// Retained artifact containing observed output values.
    pub result_artifact: String,
}
/// Environment identity required to interpret a retained Excel observation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ObservationSettings {
    /// Observed workbook date epoch.
    pub date_system: DateSystem,
    /// Observed Excel locale.
    pub locale: String,
    /// Observed Excel compatibility version.
    pub compatibility_version: u8,
}

/// Fixture spelling of the canonical workbook date epoch.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub(crate) enum DateSystem {
    /// 1900 epoch with the fictional leap-day serial.
    #[default]
    #[serde(rename = "1900")]
    Excel1900,
    /// 1904 epoch.
    #[serde(rename = "1904")]
    Excel1904,
}
impl From<DateSystem> for model::DateSystem {
    fn from(system: DateSystem) -> Self {
        match system {
            DateSystem::Excel1900 => Self::Excel1900,
            DateSystem::Excel1904 => Self::Excel1904,
        }
    }
}
/// Currency formatting contracts explicitly supported by these fixtures.
#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum Currency {
    /// US dollar formatting.
    #[serde(rename = "USD")]
    Usd,
}
/// Documented Excel text-unit contract selected by the fixture.
#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum TextPolicy {
    /// Compatibility-v1 text-unit behavior.
    #[serde(rename = "excel_v1")]
    ExcelV1,
    /// Documented compatibility-v2 surrogate-pair counting.
    #[serde(rename = "excel_v2")]
    ExcelV2,
}
/// Explicit numeric-date field ordering used by context-sensitive parsing.
#[derive(Clone, Copy, Debug, Deserialize)]
pub(crate) enum DateOrder {
    /// Month, day, then year.
    #[serde(rename = "MDY")]
    Mdy,
    /// Day, month, then year.
    #[serde(rename = "DMY")]
    Dmy,
}
/// Optional deterministic clock, random seed, and localization edits for later evaluations.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContextUpdate {
    /// Replacement captured evaluation instant.
    pub as_of_time: Option<std::time::SystemTime>,
    /// Replacement deterministic random-stream seed.
    pub randomness_seed: Option<u64>,
    /// Replacement fixed timezone offset.
    pub utc_offset_seconds: Option<i32>,
    /// Replacement numeric-date parsing order.
    pub date_order: Option<DateOrder>,
}
impl ContextUpdate {
    /// Apply only explicitly supplied context fields to the retained evaluation settings.
    pub fn apply(self, context: &mut eval::EvaluationInputs) {
        if let Some(clock) = self.as_of_time {
            context.as_of_time = clock;
        }
        if let Some(seed) = self.randomness_seed {
            context.randomness_seed = seed;
        }
        if let Some(offset) = self.utc_offset_seconds {
            context.localization.utc_offset_seconds = offset;
        }
        if let Some(order) = self.date_order {
            context.localization.date_order = match order {
                DateOrder::Mdy => eval::DateOrder::MonthDayYear,
                DateOrder::Dmy => eval::DateOrder::DayMonthYear,
            };
        }
    }
}
/// Fixture selection of scalar projection versus dynamic spill publication.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ArrayResultPolicy {
    /// Project array results to one scalar.
    #[default]
    ScalarIntersection,
    /// Publish supported rectangular results into worksheet children.
    DynamicSpill,
}
impl ArrayResultPolicy {
    /// Convert the fixture policy to the evaluator's array-publication contract.
    pub fn execution(self) -> eval::ArrayResultPolicy {
        match self {
            Self::ScalarIntersection => eval::ArrayResultPolicy::ScalarIntersection,
            Self::DynamicSpill => eval::ArrayResultPolicy::DynamicSpill,
        }
    }
}
/// Workbook epoch and initial display/text/array/context choices.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialSettings {
    /// Epoch installed before source cells and formulas are evaluated.
    #[serde(default)]
    pub date_system: DateSystem,
    /// Optional initial currency formatter.
    pub currency: Option<Currency>,
    /// Optional override of the case's text-unit contract.
    pub text_policy: Option<TextPolicy>,
    /// Optional initial array-publication contract.
    pub array_result_policy: Option<ArrayResultPolicy>,
    /// Initial deterministic context overrides.
    #[serde(default)]
    pub context: ContextUpdate,
}
/// Partial policy/context changes applied between retained overlay evaluations.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsUpdate {
    /// Replacement currency formatting contract.
    pub currency: Option<Currency>,
    /// Replacement text-unit contract.
    pub text_policy: Option<TextPolicy>,
    /// Replacement array-publication contract.
    pub array_result_policy: Option<ArrayResultPolicy>,
    /// Context fields to update for the next evaluation.
    #[serde(default)]
    pub context: ContextUpdate,
}

// Typed projection of the registry fields consumed by the runner. The complete
// coverage metadata is validated separately by the package's specification tool.
/// Typed projection of feature coverage metadata consumed by the fixture runner.
#[derive(Deserialize)]
pub(crate) struct RegistryInventory {
    /// Registry feature declarations to match with fixture cases.
    pub features: Vec<RegistryFeature>,
}
/// Feature identity and implementation status used during coverage validation.
#[derive(Deserialize)]
pub(crate) struct RegistryFeature {
    /// Registry feature spelling matched by fixtures.
    pub name: String,
    /// Current support classification from the checked inventory.
    pub implementation_status: ImplementationStatus,
}
/// Coverage inventory's declared implementation completeness.
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ImplementationStatus {
    /// Feature has a registered executable implementation.
    Registered,
    /// Feature is explicitly unsupported.
    Unsupported,
    /// Only part of the feature contract is implemented.
    Partial,
    /// Implementation status has not been established.
    Unknown,
}
