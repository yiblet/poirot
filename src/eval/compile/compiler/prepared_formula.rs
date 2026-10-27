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
