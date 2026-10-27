use super::registry_identity::RegistryIdentity;
use poirot_parsing::formula::FormulaParsePolicy;

/// Parser limits and function definitions used to compile formulas.
///
/// A workbook plan must be recompiled when this policy changes. Execution-time
/// clock, text, formatting, and builtin choices belong to [`crate::eval::RunConfiguration`].
///
/// ```
/// use poirot::eval::{FormulaCompiler, PreparationPolicy, WorkbookView};
/// use poirot::model::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1", "2*3")?;
/// let policy = PreparationPolicy::default();
/// let plan = FormulaCompiler::new().compile_workbook(&book, &policy);
/// let _view = WorkbookView::new(&book, &plan, &policy)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PreparationPolicy {
    /// Parsing grammar and preparation limits.
    pub parse: FormulaParsePolicy,
    /// Function namespace identity required during binding.
    pub registry: RegistryIdentity,
}

impl PreparationPolicy {
    /// Select parser limits and capture the registry required for extension binding.
    pub fn with_extensions(
        parse: FormulaParsePolicy,
        registry: &crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        Self {
            parse,
            registry: RegistryIdentity::with_extensions(registry),
        }
    }
}
