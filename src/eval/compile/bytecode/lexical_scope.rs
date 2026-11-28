use super::compile_error::CompileError;
use poirot_parsing::formula::{Expr, LetForm};

/// Active lexical declarations mapped to deterministic slots within one bytecode body.
#[derive(Default)]
pub(super) struct LexicalScope<'ast> {
    names: poirot_functions::scalar::ParamVec<&'ast str>,
}

impl<'ast> LexicalScope<'ast> {
    /// First slot available to a nested scope.
    pub(super) fn next_slot(&self) -> usize {
        self.names.len()
    }

    /// Add a declaration after lowering its initializer.
    pub(super) fn bind(&mut self, name: &'ast str) -> Result<(), CompileError> {
        self.names
            .try_reserve(1)
            .map_err(|_| CompileError::ResourceLimit)?;
        // SAFETY: The successful reservation admits this additional declaration.
        self.names.push(name);
        Ok(())
    }

    /// Restore the enclosing scope after lowering its nested result.
    pub(super) fn finish_scope(&mut self, base: usize) {
        self.names.truncate(base);
    }

    /// Resolve the nearest declaration of the reference spelling.
    pub(super) fn slot(&self, text: &str) -> Option<usize> {
        let name = LetForm::local_name(text);
        self.names
            .iter()
            .rposition(|candidate| crate::model::identifiers::matches(candidate, name))
    }

    /// Determine whether syntax used as a contextual reducer mentions a local name.
    pub(super) fn uses_local(&self, expression: &Expr, remaining: usize) -> bool {
        if self.names.is_empty() {
            return false;
        }
        let Some(remaining) = remaining.checked_sub(1) else {
            return true;
        };
        match expression {
            Expr::Name(name) => self.slot(name).is_some(),
            Expr::Function { args, .. } => args.iter().any(|arg| self.uses_local(arg, remaining)),
            _ => false,
        }
    }
}
