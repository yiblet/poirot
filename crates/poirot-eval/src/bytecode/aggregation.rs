//! Contextual aggregation syntax, distinct from ordinary worksheet values.
use poirot_parsing::formula::Expr;

/// Scoped identifier retained until a grouping call demands its aggregation role.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Candidate {
    /// Optional sheet qualifier used for defined-name lookup.
    pub sheet: Option<String>,
    /// Original identifier, never a text-valued aggregation shortcut.
    pub name: String,
}
/// Bounded scalar, horizontal, or vertical aggregation candidates.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    /// Number of vector rows.
    pub rows: usize,
    /// Number of vector columns.
    pub columns: usize,
    /// Row-major identifiers; worksheet values cannot inhabit this channel.
    pub members: Vec<Candidate>,
}
/// An expression entry and its independently lowered aggregation-role projection.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Entry instruction pointer, always an Enter instruction.
    pub start: usize,
    /// Typed role projection without retained syntax or executable reconstruction.
    pub plan: Plan,
}
impl Plan {
    /// Check bounded vector geometry and identifier storage after deserialization.
    pub fn valid(&self) -> bool {
        self.rows != 0
            && self.columns != 0
            && (self.rows == 1 || self.columns == 1)
            && self.rows.checked_mul(self.columns) == Some(self.members.len())
            && self.members.len() <= 100_000
            && self.members.iter().all(|n| {
                !n.name.is_empty()
                    && n.name.len() <= 32767
                    && n.sheet
                        .as_ref()
                        .is_none_or(|s| !s.is_empty() && s.len() <= 32767)
            })
    }
}
/// Fallible lowering distinguishes unsupported syntax from exhausted compilation storage.
type Lowered = Result<Option<Plan>, super::CompileError>;
fn copied(text: &str) -> Result<String, super::CompileError> {
    let mut copy = String::new();
    copy.try_reserve_exact(text.len())
        .map_err(|_| super::CompileError::ResourceLimit)?;
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
        .map_err(|_| super::CompileError::ResourceLimit)?;
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
            return Err(super::CompileError::ResourceLimit);
        }
        members
            .try_reserve(part.members.len())
            .map_err(|_| super::CompileError::ResourceLimit)?;
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
    registry: Option<&crate::extensions::ExtensionRegistry>,
) -> Option<bool> {
    let lookup = match registry.map(|r| r.external_name_lookup(name)) {
        Some(crate::external_namespace::ExternalNameLookup::Stored(stored)) => stored,
        Some(crate::external_namespace::ExternalNameLookup::UnsupportedName) => return None,
        _ => name,
    };
    let spelling = poirot_parsing::formula::FunctionName::from(lookup);
    let builtin = crate::registry::resolve_function(&spelling).map(|d| d.handler);
    let selected = if builtin.is_some() {
        builtin
    } else if registry.and_then(|r| r.resolve(lookup)).is_some() {
        None
    } else {
        crate::registry::resolve_excel_function(&spelling)
    };
    match selected {
        Some(crate::registry::FunctionId::Matrix(crate::registry::MatrixFunction::Hstack)) => {
            Some(true)
        }
        Some(crate::registry::FunctionId::Matrix(crate::registry::MatrixFunction::Vstack)) => {
            Some(false)
        }
        _ => None,
    }
}
/// Lower a standalone aggregation-role expression using the default function namespace.
pub fn lower(expression: &Expr) -> Lowered {
    lower_with_registry(expression, None)
}
/// Lower a declared aggregation role while preserving constructor extension precedence.
pub(crate) fn lower_with_registry(
    expression: &Expr,
    registry: Option<&crate::extensions::ExtensionRegistry>,
) -> Lowered {
    fn at(
        expression: &Expr,
        registry: Option<&crate::extensions::ExtensionRegistry>,
        depth: usize,
    ) -> Lowered {
        if depth >= super::DEPTH {
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
pub(crate) fn lower_tree(expression: &crate::bound::Expression) -> Lowered {
    use crate::bound::Expression as E;
    fn at(expression: &E, depth: usize) -> Lowered {
        if depth >= super::DEPTH {
            return Ok(None);
        }
        match expression {
            E::Name(name) => single(None, name),
            E::QualifiedName { sheet, name } => single(Some(sheet), name),
            E::UnresolvedFunction(arguments) => {
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
            E::Matrix(call)
                if matches!(
                    call.function,
                    crate::registry::MatrixFunction::Hstack
                        | crate::registry::MatrixFunction::Vstack
                ) =>
            {
                if !(1..=255).contains(&call.arguments.expressions.len()) {
                    return Ok(None);
                }
                combine(
                    call.function == crate::registry::MatrixFunction::Hstack,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytecode::{compile, compile_aggregation_argument};
    #[test]
    fn contextual_lowering_has_no_ordinary_name_overhead_and_admits_large_nested_vectors() {
        for formula in ["SUM", "HSTACK(RangeOne,RangeTwo)", "IF(FALSE,SUM,AVERAGE)"] {
            assert!(
                compile(&poirot_parsing::formula::parse_formula(formula).unwrap())
                    .unwrap()
                    .template
                    .aggregations
                    .is_empty()
            );
        }
        let formula = "GROUPBY({1;1},{2;4},HSTACK(SUM,AVERAGE),0,0)";
        let compiled = compile(&poirot_parsing::formula::parse_formula(formula).unwrap()).unwrap();
        assert_eq!(compiled.template.aggregations.len(), 1);
        assert_eq!(compiled.template.aggregations[0].plan.columns, 2);
        assert_eq!(
            compiled.template.aggregations[0].plan.members[0].name,
            "SUM"
        );
        let over_arity = format!("HSTACK({})", vec!["SUM"; 256].join(","));
        assert!(
            lower(&poirot_parsing::formula::parse_formula(&over_arity).unwrap())
                .unwrap()
                .is_none()
        );
        let row = format!("HSTACK({})", vec!["SUM"; 200].join(","));
        let syntax =
            poirot_parsing::formula::parse_formula(&format!("HSTACK({row},{row})")).unwrap();
        assert_eq!(
            compile_aggregation_argument(&syntax)
                .unwrap()
                .template
                .aggregations[0]
                .plan
                .columns,
            400
        );
        assert!(
            lower(&poirot_parsing::formula::parse_formula("\"SUM\"").unwrap())
                .unwrap()
                .is_none()
        );
        assert!(
            lower(
                &poirot_parsing::formula::parse_formula("HSTACK(VSTACK(SUM,MAX),AVERAGE)").unwrap()
            )
            .unwrap()
            .is_none()
        );
    }
    #[test]
    fn projection_agrees_between_inline_program_and_tree_arguments() {
        use crate::executable::ArgumentsRef;
        let syntax = poirot_parsing::formula::parse_formula("VSTACK(SUM,MEDIAN)").unwrap();
        let compiled = compile_aggregation_argument(&syntax).unwrap();
        let programs = [compiled];
        let program = ArgumentsRef::Programs(&programs)
            .get(0)
            .unwrap()
            .aggregation()
            .unwrap()
            .unwrap();
        let tree = crate::bound::bind(&syntax);
        let tree = lower_tree(&tree).unwrap().unwrap();
        assert_eq!(*program, tree);
        assert_eq!((tree.rows, tree.columns), (2, 1));
    }
}
