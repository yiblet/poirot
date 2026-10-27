//! Borrowed compilation services, independent of execution state and observers.

use super::compiler::{CompiledWorkbook, FormulaCompiler, PreparationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions;
use crate::model::CellAddress;

/// Compiles authored formulas and obtains their possible spill bounds without executing them.
/// Discovery borrows this service to classify potential array producers, reusing the
/// shared compiler when available.
pub(in crate::eval) struct SourcePreparation<'a> {
    pub(in crate::eval) extensions: Option<&'a extensions::ExtensionRegistry>,
    pub(in crate::eval) policy: Option<&'a PreparationPolicy>,
    pub(in crate::eval) compiler: Option<&'a std::cell::RefCell<FormulaCompiler>>,
}

impl SourcePreparation<'_> {
    /// Borrow the shared compiler or prepare locally if it is already in use.
    pub(in crate::eval) fn with_compiler<R>(
        &self,
        prepare: impl FnOnce(&mut FormulaCompiler, &PreparationPolicy) -> R,
    ) -> R {
        let default;
        let policy = match self.policy {
            Some(policy) => policy,
            None => {
                default = self
                    .extensions
                    .map_or_else(PreparationPolicy::default, |registry| {
                        PreparationPolicy::with_extensions(
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
    pub(in crate::eval) fn spill_shape(
        &self,
        plan: Option<&CompiledWorkbook>,
        sheet: usize,
        at: CellAddress,
        source: &str,
    ) -> Option<crate::model::ShapeBounds> {
        if let Some(prepared) = plan
            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address: at }))
            .filter(|prepared| prepared.source() == source)
        {
            return prepared.spill_shape();
        }
        self.with_compiler(|compiler, policy| compiler.spill_shape(source, policy, self.extensions))
    }
}
