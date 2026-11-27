use super::call_target::CallTarget;
use super::compile_error::CompileError;
use crate::eval::functions::extensions::ExtensionRegistry;
use poirot_parsing::formula::{Expr, LetForm};

/// Active lexical declarations mapped to deterministic slots within one bytecode body.
pub(super) struct LexicalScope<'ast, 'registry> {
    names: poirot_functions::scalar::ParamVec<&'ast str>,
    pub(super) registry: Option<&'registry ExtensionRegistry>,
}

impl<'ast, 'registry> LexicalScope<'ast, 'registry> {
    /// Start an independent scope in the same registered function namespace.
    pub(super) fn new(registry: Option<&'registry ExtensionRegistry>) -> Self {
        Self {
            names: Default::default(),
            registry,
        }
    }

    /// Select a lexical binding when no registered handler owns this spelling.
    pub(super) fn callable_slot(&self, name: &str) -> Option<usize> {
        let slot = self.slot(name)?;
        let (lookup, target) = CallTarget::resolve(name, self.registry);
        (target == CallTarget::Unresolved
            && crate::eval::functions::registry::resolve_handler(&lookup.unwrap_or(name).into())
                .is_none())
        .then_some(slot)
    }

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

    /// Borrow a declaration at a checked slot.
    pub(super) fn name(&self, slot: usize) -> Option<&'ast str> {
        self.names.get(slot).copied()
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
            Expr::Function { name, args } => {
                self.callable_slot(name).is_some()
                    || args.iter().any(|arg| self.uses_local(arg, remaining))
            }
            Expr::Invoke { callee, args } => {
                self.uses_local(callee, remaining)
                    || args.iter().any(|arg| self.uses_local(arg, remaining))
            }
            _ => false,
        }
    }
}
