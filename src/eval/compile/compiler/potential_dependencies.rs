use super::potential_qualified_name::PotentialQualifiedName;
use super::potential_reference::PotentialReference;
use crate::eval::functions::registry;

/// Conservative references and names visible in formula syntax before execution.
/// Unselected branches may appear here; use consumed-read events for actual dependencies.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PotentialDependencies {
    /// References found in syntax, independent of branch selection.
    pub references: Vec<PotentialReference>,
    /// Unqualified names found in syntax.
    pub names: Vec<String>,
    /// Qualified names found in syntax.
    pub qualified_names: Vec<PotentialQualifiedName>,
    /// Unresolved call spellings whose behavior cannot be statically described.
    pub opaque_functions: Vec<String>,
    /// Whether a handler can choose references at runtime.
    pub dynamic_references: bool,
    /// Extension bindings captured in resolved bytecode calls.
    pub extensions: Vec<crate::eval::functions::extensions::ExtensionBinding>,
}

pub(super) fn potential(
    expression: &poirot_parsing::formula::Expr,
    executable: &crate::eval::compile::bytecode::Compiled,
) -> Option<PotentialDependencies> {
    use poirot_parsing::formula::Expr;
    let mut result = PotentialDependencies::default();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match expression {
            Expr::Cell(reference) => result
                .references
                .push(PotentialReference::Cell(reference.clone())),
            Expr::Column {
                column,
                absolute,
                sheet,
            } => result.references.push(PotentialReference::Column {
                column: *column,
                absolute: *absolute,
                sheet: sheet.clone(),
            }),
            Expr::Row {
                row,
                absolute,
                sheet,
            } => result.references.push(PotentialReference::Row {
                row: *row,
                absolute: *absolute,
                sheet: sheet.clone(),
            }),
            Expr::SheetSpan(span) => result
                .references
                .push(PotentialReference::SheetSpan(span.clone())),
            Expr::Name(name) => result.names.push(name.clone()),
            Expr::QualifiedName { sheet, name } => {
                result.qualified_names.push(PotentialQualifiedName {
                    sheet: sheet.clone(),
                    name: name.clone(),
                });
            }
            Expr::Function { name, args } => {
                match registry::resolve_function(name) {
                    None => {}
                    Some(descriptor)
                        if descriptor
                            .dependencies
                            .contains(registry::DependencyPolicy::REFERENCE_SELECTION) =>
                    {
                        result.dynamic_references = true
                    }
                    Some(_) => {}
                }
                pending.extend(args);
            }
            Expr::Unary { operand, .. } => pending.push(operand),
            Expr::Binary { left, right, .. } => {
                pending.push(left);
                pending.push(right);
            }
            Expr::Array(rows) => pending.extend(rows.iter().flatten()),
            _ => {}
        }
    }
    // Resolved call identities distinguish declared providers from opaque calls.
    for (index, call) in executable.calls.iter().enumerate() {
        match &call.target {
            crate::eval::compile::bytecode::CallTarget::Extension(binding) => {
                result.extensions.push(binding.clone())
            }
            crate::eval::compile::bytecode::CallTarget::Unresolved => {
                result
                    .opaque_functions
                    .push(executable.source_spelling(index).ok()?.to_owned());
            }
            _ => {}
        }
    }
    Some(result)
}
