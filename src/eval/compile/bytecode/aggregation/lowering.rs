//! Lowering of aggregation-role syntax into bounded identifier plans.
use super::candidate::Candidate;
use super::plan::Plan;
use poirot_parsing::formula::Expr;

/// Fallible lowering distinguishes unsupported syntax from exhausted compilation storage.
type Lowered = Result<Option<Plan>, super::super::compile_error::CompileError>;

fn copied(text: &str) -> Result<String, super::super::compile_error::CompileError> {
    let mut copy = String::new();
    copy.try_reserve_exact(text.len())
        .map_err(|_| super::super::compile_error::CompileError::ResourceLimit)?;
    // SAFETY: The successful local reservation admits every byte of this unchanged string.
    copy.push_str(text);
    Ok(copy)
}

fn single(sheet: Option<&str>, name: &str) -> Lowered {
    if name.is_empty()
        || name.len() > 32767
        || sheet.is_some_and(|s| s.is_empty() || s.len() > 32767)
    {
        return Ok(None);
    }
    let mut members = Vec::new();
    members
        .try_reserve_exact(1)
        .map_err(|_| super::super::compile_error::CompileError::ResourceLimit)?;
    // SAFETY: One slot was reserved locally and the vector is still empty.
    members.push(Candidate {
        sheet: sheet.map(copied).transpose()?,
        name: copied(name)?,
    });
    Ok(Some(Plan {
        rows: 1,
        columns: 1,
        members,
    }))
}

fn combine(horizontal: bool, parts: impl IntoIterator<Item = Lowered>) -> Lowered {
    let mut members = Vec::new();
    for part in parts {
        let Some(part) = part? else {
            return Ok(None);
        };
        if horizontal && part.rows != 1 || !horizontal && part.columns != 1 {
            return Ok(None);
        }
        if members
            .len()
            .checked_add(part.members.len())
            .is_none_or(|n| n > 100_000)
        {
            return Err(super::super::compile_error::CompileError::ResourceLimit);
        }
        members
            .try_reserve(part.members.len())
            .map_err(|_| super::super::compile_error::CompileError::ResourceLimit)?;
        // SAFETY: The successful reservation admits exactly the unchanged source member count.
        members.extend(part.members);
    }
    let n = members.len();
    let plan = Plan {
        rows: if horizontal { 1 } else { n },
        columns: if horizontal { n } else { 1 },
        members,
    };
    Ok(plan.valid().then_some(plan))
}

fn constructor(
    name: &str,
    registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
) -> Option<bool> {
    let lookup = match registry.map(|r| r.external_name_lookup(name)) {
        Some(crate::eval::functions::external_namespace::ExternalNameLookup::Stored(stored)) => {
            stored
        }
        Some(crate::eval::functions::external_namespace::ExternalNameLookup::UnsupportedName) => {
            return None;
        }
        _ => name,
    };
    let spelling = poirot_parsing::formula::FunctionName::from(lookup);
    let builtin = crate::eval::functions::registry::resolve_function(&spelling).map(|d| d.handler);
    let selected = if builtin.is_some() {
        builtin
    } else if registry.and_then(|r| r.resolve(lookup)).is_some() {
        None
    } else {
        crate::eval::functions::registry::resolve_handler(&spelling)
    };
    match selected {
        Some(crate::eval::functions::registry::FunctionId::Matrix(
            crate::eval::functions::registry::MatrixFunction::Hstack,
        )) => Some(true),
        Some(crate::eval::functions::registry::FunctionId::Matrix(
            crate::eval::functions::registry::MatrixFunction::Vstack,
        )) => Some(false),
        _ => None,
    }
}

/// Lower a standalone aggregation-role expression using the default function namespace.
pub fn lower(expression: &Expr) -> Lowered {
    lower_with_registry(expression, None)
}

/// Lower a declared aggregation role while preserving constructor extension precedence.
pub(in crate::eval) fn lower_with_registry(
    expression: &Expr,
    registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
) -> Lowered {
    fn at(
        expression: &Expr,
        registry: Option<&crate::eval::functions::extensions::ExtensionRegistry>,
        depth: usize,
    ) -> Lowered {
        if depth >= super::super::lowering::DEPTH {
            return Ok(None);
        }
        match expression {
            Expr::Name(name) => single(None, name),
            Expr::QualifiedName { sheet, name } => single(Some(sheet), name),
            Expr::Function { name, args } => {
                if !(1..=255).contains(&args.len()) {
                    return Ok(None);
                }
                let Some(horizontal) = constructor(name, registry) else {
                    return Ok(None);
                };
                combine(
                    horizontal,
                    args.iter()
                        .map(|a| at(a, registry, depth.saturating_add(1))),
                )
            }
            _ => Ok(None),
        }
    }
    at(expression, registry, 0)
}

#[cfg(test)]
/// Lower the independent bound-tree oracle through the same restricted role grammar.
pub(in crate::eval) fn lower_tree(expression: &crate::eval::compile::bound::Expression) -> Lowered {
    use crate::eval::compile::bound::Expression;
    fn at(expression: &Expression, depth: usize) -> Lowered {
        if depth >= super::super::lowering::DEPTH {
            return Ok(None);
        }
        match expression {
            Expression::Name(name) => single(None, name),
            Expression::QualifiedName { sheet, name } => single(Some(sheet), name),
            Expression::UnresolvedFunction(arguments) => {
                if !(1..=255).contains(&arguments.expressions.len()) {
                    return Ok(None);
                }
                let Some(horizontal) = constructor(arguments.resolved_spelling(), None) else {
                    return Ok(None);
                };
                combine(
                    horizontal,
                    arguments
                        .expressions
                        .iter()
                        .map(|a| at(a, depth.saturating_add(1))),
                )
            }
            Expression::Matrix(call)
                if matches!(
                    call.function,
                    crate::eval::functions::registry::MatrixFunction::Hstack
                        | crate::eval::functions::registry::MatrixFunction::Vstack
                ) =>
            {
                if !(1..=255).contains(&call.arguments.expressions.len()) {
                    return Ok(None);
                }
                combine(
                    call.function == crate::eval::functions::registry::MatrixFunction::Hstack,
                    call.arguments
                        .expressions
                        .iter()
                        .map(|a| at(a, depth.saturating_add(1))),
                )
            }
            _ => Ok(None),
        }
    }
    at(expression, 0)
}
