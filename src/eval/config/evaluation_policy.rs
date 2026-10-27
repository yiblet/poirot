use super::array_result_policy::ArrayResultPolicy;
use super::builtin_semantics_policy::BuiltinSemanticsPolicy;
use super::producer_policy::ProducerPolicy;
use super::run_configuration::RunConfiguration;

/// Parsing and computation rules owned by Overlay or a direct Evaluation workspace.
/// Compilation derives a [`crate::eval::PreparationPolicy`] from the parse settings and
/// extension registry; execution choices remain in this policy.
///
/// ```
/// use poirot::eval::ArrayResultPolicy;
/// use poirot::eval::{EvaluationContext, EvaluationPolicy, overlay::Evaluation};
/// use poirot::model::{CellValue, Workbook};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "{2,3}")?;
/// let policy = EvaluationPolicy { arrays: ArrayResultPolicy::DynamicSpill, ..Default::default() };
/// let mut run = Evaluation::with_workbook(&book).with_policy(policy)
///     .build(EvaluationContext::default())?;
/// assert_eq!(run.cell(run.target("Data!B1")?)?, CellValue::Number(3.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EvaluationPolicy {
    /// Formula grammar and preparation limits.
    pub parse: poirot_parsing::formula::FormulaParsePolicy,
    /// Selected builtin compatibility behavior.
    pub builtin: BuiltinSemanticsPolicy,
    /// Rules for returning and publishing array results.
    pub arrays: ArrayResultPolicy,
    /// Compute possible producers or trust saved source results.
    pub producers: ProducerPolicy,
    /// Text comparison and coercion behavior.
    pub text: poirot_functions::text_units::TextSemanticsPolicy,
    /// Conversion rules when exporting evaluated values into workbook caches.
    pub materialization: crate::eval::materialization::FormulaCacheMaterializationPolicy,
}

impl Default for EvaluationPolicy {
    fn default() -> Self {
        let configuration = RunConfiguration::default();
        Self {
            parse: Default::default(),
            builtin: configuration.builtin_policy(),
            arrays: configuration.array_result_policy(),
            producers: configuration.producer_policy(),
            text: configuration.text_policy(),
            materialization: configuration.materialization_policy(),
        }
    }
}
