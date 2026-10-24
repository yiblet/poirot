//! Borrowed compilation services, independent of execution state and observers.
use super::*;

/// Exact-source compiler and its policy, without workbook execution capabilities.
pub(super) struct SourcePreparation<'a> {
    pub(super) extensions: Option<&'a extensions::ExtensionRegistry>,
    pub(super) policy: Option<&'a EvaluationPolicy>,
    pub(super) compiler: Option<&'a std::cell::RefCell<FormulaCompiler>>,
}
impl SourcePreparation<'_> {
    /// Borrow the shared compiler or prepare locally if it is already in use.
    pub(super) fn with_compiler<R>(
        &self,
        prepare: impl FnOnce(&mut FormulaCompiler, &EvaluationPolicy) -> R,
    ) -> R {
        let default;
        let policy = match self.policy {
            Some(policy) => policy,
            None => {
                default = self
                    .extensions
                    .map_or_else(EvaluationPolicy::default, |registry| {
                        EvaluationPolicy::with_extensions(
                            poirot_parsing::formula::FormulaParsePolicy::default(),
                            registry,
                        )
                    });
                &default
            }
        };
        if let Some(mut compiler) = self
            .compiler
            .and_then(|compiler| compiler.try_borrow_mut().ok())
        {
            return prepare(&mut compiler, policy);
        }
        prepare(&mut FormulaCompiler::new(), policy)
    }
    /// Read formal source geometry; preparation failures cannot prove exclusion.
    pub(super) fn spill_shape(
        &self,
        plan: Option<&CompiledWorkbook>,
        sheet: usize,
        at: CellAddress,
        source: &str,
    ) -> Option<poirot_model::ShapeBounds> {
        if let Some(prepared) = plan
            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address: at }))
            .filter(|prepared| prepared.source() == source)
        {
            return prepared.spill_shape();
        }
        self.with_compiler(|compiler, policy| compiler.spill_shape(source, policy, self.extensions))
    }
}
