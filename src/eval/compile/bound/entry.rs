use super::binding;
use super::domain::Expression;
use poirot_parsing::formula::Expr as Syntax;

/// Internal parser adapter bounds individual subtrees, keeping errors deferred in lazy branches.
#[cfg(test)]
pub(in crate::eval) fn bind(syntax: &Syntax) -> Expression {
    bind_with_registry(syntax, None)
}

/// Compile an extension-aware executable subtree with selected-branch validation.
#[cfg(test)]
pub(in crate::eval) fn bind_with_extensions_deferred(
    syntax: &Syntax,
    registry: &crate::eval::functions::extensions::ExtensionRegistry,
) -> Expression {
    binding::bind_at(syntax, 0, Some(registry))
}

/// Bind syntax against an optional explicit extension namespace.
#[cfg(test)]
pub(in crate::eval) fn bind_with_registry(
    syntax: &Syntax,
    registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
) -> Expression {
    match registry {
        Some(registry) => bind_with_extensions_deferred(syntax, registry),
        None => binding::bind_at(syntax, 0, None),
    }
}

/// Reconstruct an introspection tree using call identities already captured in bytecode.
pub(in crate::eval) fn bind_with_executable(
    syntax: &Syntax,
    executable: &crate::eval::compile::bytecode::Executable,
) -> Expression {
    binding::bind_at(syntax, 0, Some(executable))
}
