use crate::eval::compile::SourcePreparation;
use crate::eval::compile::compiler::{
    CompiledWorkbook, FormulaCompiler, PreparationPolicy, PreparedFormula,
};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions::ExtensionRegistry;
use crate::eval::inputs::{EffectiveWorkbook, RunInputs};
use crate::eval::values::CellEvaluation;
use crate::model::{CellAddress, CellValue, ShapeBounds, Workbook};
use std::cell::RefCell;
use std::sync::Arc;

/// Workbook content, scenario inputs and compilation services one run reads from.
pub(in crate::eval) struct RunSources<'a> {
    /// Authored workbook the run evaluates.
    pub(in crate::eval) workbook: &'a Workbook,
    /// Sparse scenario overrides layered over the workbook.
    pub(in crate::eval) inputs: Option<&'a RunInputs>,
    /// Compiled programs shared by an overlay, when available.
    pub(in crate::eval) plan: Option<&'a CompiledWorkbook>,
    /// Shared compiler for formulas reached without a compiled program.
    pub(in crate::eval) lazy_compiler: Option<&'a RefCell<FormulaCompiler>>,
    /// Preparation policy paired with the shared compiler.
    pub(in crate::eval) preparation_policy: Option<&'a PreparationPolicy>,
    /// Registered custom functions visible to compiled formulas.
    pub(in crate::eval) extensions: Option<&'a ExtensionRegistry>,
}

impl<'a> RunSources<'a> {
    /// Read a workbook without overrides, compiled plans or extensions.
    pub(in crate::eval) fn new(workbook: &'a Workbook) -> Self {
        Self {
            workbook,
            inputs: None,
            plan: None,
            lazy_compiler: None,
            preparation_policy: None,
            extensions: None,
        }
    }

    /// Pair a calculated value with the effective cell formatting of its target.
    pub(in crate::eval) fn output(
        &self,
        target: EvaluationTarget,
        value: CellValue,
    ) -> CellEvaluation {
        let formatting = EffectiveWorkbook::new(self.workbook, self.inputs)
            .cell(target.sheet, target.address)
            .map(|cell| &cell.style)
            .filter(|style| style.number_format.is_some() || style.visual.is_some())
            .cloned();
        CellEvaluation {
            target,
            value,
            formatting,
        }
    }

    /// Compile a reached source using the overlay's shared compiler when available.
    pub(in crate::eval) fn prepare_source(&self, source: &str) -> Arc<PreparedFormula> {
        self.with_source_compiler(|compiler, policy| match self.extensions {
            Some(registry) => compiler.compile_formula_with_extensions(source, policy, registry),
            None => compiler.compile_formula(source, policy),
        })
    }

    /// Borrow the shared source compiler and preparation policy, falling back locally.
    pub(in crate::eval) fn with_source_compiler<R>(
        &self,
        prepare: impl FnOnce(&mut FormulaCompiler, &PreparationPolicy) -> R,
    ) -> R {
        self.source_preparation().with_compiler(prepare)
    }

    /// Borrow only source-compilation services for discovery and reached formulas.
    pub(in crate::eval) fn source_preparation(&self) -> SourcePreparation<'a> {
        SourcePreparation {
            extensions: self.extensions,
            policy: self.preparation_policy,
            compiler: self.lazy_compiler,
        }
    }

    /// Borrow or prepare a source formula using the existing exact-source compiler cache.
    pub(in crate::eval) fn source_formula(
        &self,
        sheet: usize,
        at: CellAddress,
    ) -> Option<Arc<PreparedFormula>> {
        let source = EffectiveWorkbook::new(self.workbook, self.inputs)
            .cell(sheet, at)?
            .formula
            .as_deref()?;
        self.plan
            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address: at }))
            .filter(|prepared| prepared.source() == source)
            .cloned()
            .or_else(|| Some(self.prepare_source(source)))
    }

    /// Read formal source geometry without evaluating the producer.
    pub(in crate::eval) fn source_spill_shape(
        &self,
        sheet: usize,
        at: CellAddress,
        source: &str,
    ) -> Option<ShapeBounds> {
        self.source_preparation()
            .spill_shape(self.plan, sheet, at, source)
    }
}
