use super::admission::validate_size;
use super::binding_error::BindingError;
use super::domain::Expression;
use poirot_parsing::formula::Expr as Syntax;

/// Caller-owned syntax plus immutable bytecode; bound inspection views are constructed on demand.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaPlan {
    /// Original syntax retained for caller-requested inspection.
    pub(super) syntax: Syntax,
    /// The sole production execution representation.
    pub(super) bytecode: crate::eval::compile::bytecode::Compiled,
    /// Registry identity required by bound extension calls.
    pub(super) extension_identity: Option<crate::eval::functions::extensions::ExtensionIdentity>,
}

impl FormulaPlan {
    /// Validate and compile caller-owned syntax without extensions.
    pub fn bind(syntax: &Syntax) -> Result<Self, BindingError> {
        validate_size(syntax)?;
        Ok(Self {
            syntax: syntax.clone(),
            bytecode: crate::eval::compile::bytecode::compile(syntax)
                .map_err(BindingError::from)?,
            extension_identity: None,
        })
    }

    /// Validate and bind syntax while capturing the exact extension registry identity.
    pub fn bind_with_extensions(
        syntax: &Syntax,
        registry: &crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Result<Self, BindingError> {
        validate_size(syntax)?;
        Ok(Self {
            syntax: syntax.clone(),
            bytecode: crate::eval::compile::bytecode::compile_with_registry(syntax, Some(registry))
                .map_err(BindingError::from)?,
            extension_identity: Some(registry.identity()),
        })
    }

    /// Borrow the immutable bytecode used by production execution.
    pub fn bytecode(&self) -> &crate::eval::compile::bytecode::Compiled {
        &self.bytecode
    }

    /// Return the extension registry identity captured during binding.
    pub fn extension_identity(
        &self,
    ) -> Option<&crate::eval::functions::extensions::ExtensionIdentity> {
        self.extension_identity.as_ref()
    }

    /// Borrow the original parsed syntax.
    pub fn syntax(&self) -> &Syntax {
        &self.syntax
    }

    /// Construct an owned handler-bound inspection view without retaining an execution tree.
    pub fn expression(&self) -> Expression {
        super::entry::bind_with_executable(&self.syntax, &self.bytecode.executable)
    }
}
