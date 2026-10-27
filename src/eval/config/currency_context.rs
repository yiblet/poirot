/// Owned separators, grouping pattern, and sign style used by currency-formatting formulas.
/// Execution borrows this explicit locale description instead of reading the host
/// locale or requiring static strings.
///
/// ```
/// use poirot::eval::{CurrencyContext, EvaluationContext, FormattingConfiguration};
/// use poirot::eval::overlay::Evaluation;
/// use poirot::model::{CellValue, Workbook};
/// let currency = CurrencyContext {
///     decimal_separator: ".".into(), grouping_separator: ",".into(),
///     primary_group: 3, secondary_group: 3,
///     positive_prefix: "$".into(), positive_suffix: "".into(),
///     negative_prefix: "($".into(), negative_suffix: ")".into(),
/// };
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "DOLLAR(12.5,2)")?;
/// let context = EvaluationContext {
///     formatting: FormattingConfiguration::with_currency(currency), ..Default::default()
/// };
/// let mut run = Evaluation::with_workbook(&book).build(context)?;
/// let output = run.target("Data!A1")?;
/// assert_eq!(run.cell(output)?, CellValue::Text("$12.50".into()));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct CurrencyContext {
    /// Separator between integer and fractional digits.
    pub decimal_separator: String,
    /// Separator inserted between integer digit groups.
    pub grouping_separator: String,
    /// Size of the rightmost integer digit group.
    pub primary_group: usize,
    /// Size of subsequent integer digit groups.
    pub secondary_group: usize,
    /// Affix preceding nonnegative amounts.
    pub positive_prefix: String,
    /// Affix following nonnegative amounts.
    pub positive_suffix: String,
    /// Affix preceding negative amounts.
    pub negative_prefix: String,
    /// Affix following negative amounts.
    pub negative_suffix: String,
}

impl CurrencyContext {
    /// Borrow these owned settings as the financial helper's formatting descriptor.
    pub fn as_format(&self) -> poirot_functions::financial::CurrencyFormat<'_> {
        poirot_functions::financial::CurrencyFormat {
            decimal_separator: &self.decimal_separator,
            grouping_separator: &self.grouping_separator,
            primary_group: self.primary_group,
            secondary_group: self.secondary_group,
            positive_prefix: &self.positive_prefix,
            positive_suffix: &self.positive_suffix,
            negative_prefix: &self.negative_prefix,
            negative_suffix: &self.negative_suffix,
        }
    }
}

impl From<poirot_functions::financial::CurrencyFormat<'_>> for CurrencyContext {
    fn from(f: poirot_functions::financial::CurrencyFormat<'_>) -> Self {
        Self {
            decimal_separator: f.decimal_separator.into(),
            grouping_separator: f.grouping_separator.into(),
            primary_group: f.primary_group,
            secondary_group: f.secondary_group,
            positive_prefix: f.positive_prefix.into(),
            positive_suffix: f.positive_suffix.into(),
            negative_prefix: f.negative_prefix.into(),
            negative_suffix: f.negative_suffix.into(),
        }
    }
}
