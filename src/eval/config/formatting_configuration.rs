use super::currency_context::CurrencyContext;

/// Caller-supplied currency formatting settings for formula execution.
/// Default leaves currency unspecified; the evaluator does not read the host locale.
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct FormattingConfiguration {
    currency: Option<CurrencyContext>,
}

impl FormattingConfiguration {
    /// Install caller-supplied currency formatting settings.
    pub fn with_currency(currency: CurrencyContext) -> Self {
        Self {
            currency: Some(currency),
        }
    }

    /// Borrow currency settings, or `None` when the caller did not supply them.
    pub fn currency(&self) -> Option<&CurrencyContext> {
        self.currency.as_ref()
    }
}
