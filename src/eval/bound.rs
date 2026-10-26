//! Caller-owned bytecode plans with typed, owned inspection views.
//! Raw syntax remains generic and is retained for serialization/diagnostics.
//!
//! ```
//! use poirot::eval::{bound::{FormulaPlan, Expression}, registry::SearchFunction};
//! use poirot::model::{Workbook, CellAddress, CellValue};
//! let syntax = poirot_parsing::formula::parse_formula("VLOOKUP(2,{1,10;2,20},2,FALSE)")?;
//! let plan = FormulaPlan::bind(&syntax)?;
//! if let Expression::Search(search) = plan.expression() {
//!     assert_eq!(search.function, SearchFunction::VLookup);
//! }
//! let mut workbook = Workbook::new();
//! workbook.add_sheet("Sheet1")?;
//! assert_eq!(match poirot::eval::run_plan(&workbook, 0, CellAddress::new(0,0)?, &plan, Default::default(), Default::default()) { poirot::eval::EvaluationOutcome::Complete { value, .. } => value, outcome => panic!("evaluation aborted: {outcome:?}"), }, CellValue::Number(20.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod admission;
mod binding;
mod domain;

/// Structural admission failures for manually supplied syntax.
pub use admission::BindingError;
// Bound inspection expressions, family metadata, and bytecode-backed formula plans.
pub use domain::*;
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
    registry: &crate::eval::extensions::ExtensionRegistry,
) -> Expression {
    binding::bind_at(syntax, 0, Some(registry))
}
/// Bind syntax against an optional explicit extension namespace.
#[cfg(test)]
pub(in crate::eval) fn bind_with_registry(
    syntax: &Syntax,
    registry: Option<&crate::eval::extensions::ExtensionRegistry>,
) -> Expression {
    match registry {
        Some(registry) => bind_with_extensions_deferred(syntax, registry),
        None => binding::bind_at(syntax, 0, None),
    }
}

#[cfg(test)]
mod tests;

/// Binding-only resolver permits lazy compatibility trees without provider capture.
pub(in crate::eval) trait BindingResolver {
    /// Recover preserved external spelling before extension lookup.
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::external_namespace::ExternalNameLookup<'a>;
    /// Resolve an exact extension binding without capturing its provider.
    fn resolve(&self, name: &str) -> Option<crate::eval::extensions::ExtensionBinding>;
}
impl BindingResolver for crate::eval::extensions::ExtensionRegistry {
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::external_namespace::ExternalNameLookup<'a> {
        self.external_name_lookup(name)
    }
    fn resolve(&self, name: &str) -> Option<crate::eval::extensions::ExtensionBinding> {
        self.resolve(name)
    }
}
/// Reconstruct an introspection tree using call identities already captured in bytecode.
pub(in crate::eval) fn bind_with_executable(
    syntax: &Syntax,
    executable: &crate::eval::bytecode::Executable,
) -> Expression {
    binding::bind_at(syntax, 0, Some(executable))
}
impl BindingResolver for crate::eval::bytecode::Executable {
    fn external_name_lookup<'a>(
        &'a self,
        name: &'a str,
    ) -> crate::eval::external_namespace::ExternalNameLookup<'a> {
        use crate::eval::external_namespace::ExternalNameLookup;
        self.calls
            .iter()
            .find_map(|call| {
                let start = call.source_offset as usize;
                let header_end = start.checked_add(4)?;
                let length = u32::from_le_bytes(
                    self.template
                        .operands
                        .get(start..header_end)?
                        .try_into()
                        .ok()?,
                ) as usize;
                let source = self
                    .template
                    .operands
                    .get(header_end..header_end.checked_add(length)?)?;
                (source == name.as_bytes()).then(|| {
                    if matches!(call.target, crate::eval::bytecode::CallTarget::Unresolved)
                        && call.resolved_name.is_none()
                    {
                        ExternalNameLookup::UnsupportedName
                    } else {
                        ExternalNameLookup::Stored(call.resolved_name.as_deref().unwrap_or(name))
                    }
                })
            })
            .unwrap_or(ExternalNameLookup::UnsupportedName)
    }
    fn resolve(&self, name: &str) -> Option<crate::eval::extensions::ExtensionBinding> {
        self.calls.iter().find_map(|call| match &call.target {
            crate::eval::bytecode::CallTarget::Extension(binding) => {
                let start = call.source_offset as usize;
                let header_end = start.checked_add(4)?;
                let length = u32::from_le_bytes(
                    self.template
                        .operands
                        .get(start..header_end)?
                        .try_into()
                        .ok()?,
                ) as usize;
                let source_name = std::str::from_utf8(
                    self.template
                        .operands
                        .get(header_end..header_end.checked_add(length)?)?,
                )
                .ok()?;
                (call.resolved_name.as_deref().unwrap_or(source_name) == name)
                    .then(|| binding.clone())
            }
            _ => None,
        })
    }
}
