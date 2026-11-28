use super::compiled_formula::CompiledFormula;
use super::preparation_policy::PreparationPolicy;
use poirot_parsing::formula::ParseError;
use std::sync::Arc;

/// A preparation failure is deferred until its cell or name is actually evaluated.
#[derive(Clone, Debug)]
pub enum PreparedFormula {
    /// Successfully compiled formula shared across matching sources and policies.
    Ready(Arc<CompiledFormula>),
    /// Deferred parse failure, raised only if execution reaches this source.
    ParseFailure {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Preparation policy under which parsing failed.
        policy: PreparationPolicy,
        /// Parser diagnostic.
        error: ParseError,
    },
    /// Bytecode preparation failed. This operational failure is deferred until reached.
    CompilationFailure {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Policy used while lowering.
        policy: PreparationPolicy,
        /// Structural or resource diagnostic.
        error: crate::eval::compile::bytecode::CompileError,
    },
    /// Preparation lacked the registry specified by its policy.
    RegistryMismatch {
        /// Exact submitted formula text.
        source: Arc<str>,
        /// Policy identifying the required registry.
        policy: PreparationPolicy,
    },
}

impl PreparedFormula {
    /// Conservative producer geometry; syntax failures cannot produce array children.
    pub(in crate::eval) fn spill_shape(&self) -> Option<crate::model::ShapeBounds> {
        match self {
            Self::Ready(formula) => formula
                .properties()
                .filter(|p| p.result.may_spill())
                .map(|p| p.shape),
            Self::ParseFailure { error, .. } => (error.kind()
                != poirot_parsing::formula::ParseErrorKind::Syntax)
                .then_some(crate::model::ShapeBounds::UNKNOWN),
            Self::CompilationFailure { .. } | Self::RegistryMismatch { .. } => {
                Some(crate::model::ShapeBounds::UNKNOWN)
            }
        }
    }

    /// Exclude a root unresolved call only when the scoped workbook cannot supply it.
    pub(in crate::eval) fn scoped_spill_shape(
        &self,
        workbook: &crate::model::Workbook,
        sheet: usize,
    ) -> Option<crate::model::ShapeBounds> {
        use crate::eval::compile::bytecode::{CallTarget, Opcode};
        use crate::eval::functions::registry;

        let shape = self.spill_shape()?;
        if let Self::Ready(formula) = self {
            let program = formula.bytecode();
            if let Some(instruction) = program.template.code.get(1)
                && instruction.opcode == Opcode::Invoke
                && let Some(call) = program.calls.get(instruction.a as usize)
                && call.target == CallTarget::Unresolved
                && let Ok(spelling) = program.resolved_spelling(instruction.a as usize)
                && registry::resolve_handler(&spelling.into()).is_none()
                && let Ok(name) = program.source_spelling(instruction.a as usize)
                && matches!(workbook.try_defined_name(name, sheet), Ok(None))
            {
                return None;
            }
        }
        Some(shape)
    }

    /// Return the exact source text for either a program or a deferred failure.
    pub fn source(&self) -> &str {
        match self {
            Self::Ready(formula) => &formula.source,
            Self::ParseFailure { source, .. }
            | Self::CompilationFailure { source, .. }
            | Self::RegistryMismatch { source, .. } => source,
        }
    }

    /// Return the policy captured during preparation.
    pub fn policy(&self) -> &PreparationPolicy {
        match self {
            Self::Ready(formula) => &formula.policy,
            Self::ParseFailure { policy, .. }
            | Self::CompilationFailure { policy, .. }
            | Self::RegistryMismatch { policy, .. } => policy,
        }
    }
}
