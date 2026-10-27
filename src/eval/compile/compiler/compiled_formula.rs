use super::potential_dependencies::PotentialDependencies;
use super::preparation_policy::PreparationPolicy;
use poirot_parsing::formula::{ParseError, ParsedFormula, parse_formula_with_policy};
use std::sync::Arc;

/// Prepared source, reusable bytecode, and possible dependencies shared by plans.
/// Syntax inspection returns an independently owned parse view. Contains no evaluated result;
/// name/reference resolution still needs a workbook run.
#[derive(Clone, Debug)]
pub struct CompiledFormula {
    pub(super) source: Arc<str>,
    pub(super) bytecode: crate::eval::compile::bytecode::Compiled,
    pub(super) potential: PotentialDependencies,
    pub(super) policy: PreparationPolicy,
}

impl CompiledFormula {
    /// Root possible-result type and conservative characteristics for this bound source.
    pub fn properties(&self) -> Option<crate::eval::compile::typing::ExpressionProperties> {
        self.bytecode.typing.root()
    }

    /// Parse an independently owned inspection view on demand. Execution retains no tree.
    /// This diagnostic work is not included in [`CompilerUsage`](super::CompilerUsage).
    pub fn parsed(&self) -> Result<ParsedFormula, ParseError> {
        parse_formula_with_policy(&self.source, self.policy.parse)
    }

    #[cfg(test)]
    pub(super) fn executable(&self) -> crate::eval::compile::bound::Expression {
        crate::eval::compile::bound::bind_with_executable(
            self.parsed().unwrap().expression(),
            &self.bytecode.executable,
        )
    }

    /// The immutable executable used for every successful preparation.
    pub fn bytecode(&self) -> &crate::eval::compile::bytecode::Compiled {
        &self.bytecode
    }

    /// Inspect conservative syntax guards; these are not consumed-read traces.
    pub fn potential_dependencies(&self) -> &PotentialDependencies {
        &self.potential
    }
}
