//! Typed binding adapter; unknown functions and runtime arity failures stay deferred.
use super::domain::{
    Arguments, CriteriaExpr, DatabaseExpr, DateExpr, EngineeringExpr, Expression, FinancialExpr,
    InformationExpr, LogicalExpr, MathExpr, MatrixExpr, ReferenceExpr, SearchExpr, StatisticsExpr,
    TextExpr,
};
use crate::eval::functions::registry::{FunctionId, resolve_function};
use poirot_parsing::formula::Expr as Syntax;

/// Bind one subtree, deferring unsupported depth and unknown calls until selected.
pub(super) fn bind_at(
    syntax: &Syntax,
    depth: usize,
    registry: Option<&dyn crate::eval::functions::extensions::BindingResolver>,
) -> Expression {
    if depth >= 128 {
        return Expression::Error("#NUM!".into());
    }
    match syntax {
        Syntax::Number(n) => Expression::Number(*n),
        Syntax::String(s) => Expression::String(s.clone()),
        Syntax::Bool(b) => Expression::Bool(*b),
        Syntax::Error(e) => Expression::Error(e.clone()),
        Syntax::Missing => Expression::Missing,
        Syntax::Cell(c) => Expression::Cell(c.clone()),
        Syntax::SheetSpan(span) => Expression::SheetSpan(span.clone()),
        Syntax::Column {
            column,
            absolute,
            sheet,
        } => Expression::Column {
            column: *column,
            absolute: *absolute,
            sheet: sheet.clone(),
        },
        Syntax::Row {
            row,
            absolute,
            sheet,
        } => Expression::Row {
            row: *row,
            absolute: *absolute,
            sheet: sheet.clone(),
        },
        Syntax::Name(n) => Expression::Name(n.clone()),
        Syntax::QualifiedName { sheet, name } => Expression::QualifiedName {
            sheet: sheet.clone(),
            name: name.clone(),
        },
        Syntax::Unary { op, operand } => Expression::Unary {
            op: *op,
            operand: Box::new(bind_at(operand, depth + 1, registry)),
        },
        Syntax::Binary { op, left, right } => Expression::Binary {
            op: *op,
            left: Box::new(bind_at(left, depth + 1, registry)),
            right: Box::new(bind_at(right, depth + 1, registry)),
        },
        Syntax::Array(rows) => Expression::Array(
            rows.iter()
                .map(|r| r.iter().map(|e| bind_at(e, depth + 1, registry)).collect())
                .collect(),
        ),
        Syntax::Function { name, args } => {
            let arguments = Arguments::new(
                name.clone(),
                args.iter()
                    .map(|e| bind_at(e, depth + 1, registry))
                    .collect(),
            );
            let lookup_name = match registry
                .map(|registry| registry.external_name_lookup(name.as_str()))
            {
                Some(crate::eval::functions::external_namespace::ExternalNameLookup::Stored(
                    stored,
                )) => stored,
                Some(
                    crate::eval::functions::external_namespace::ExternalNameLookup::UnsupportedName,
                ) => {
                    return Expression::UnresolvedFunction(arguments);
                }
                _ => name.as_str(),
            };
            let arguments = arguments.with_resolved_spelling(lookup_name);
            let resolved = if lookup_name == name.as_str() {
                resolve_function(name)
            } else {
                resolve_function(&poirot_parsing::formula::FunctionName::from(lookup_name))
            };
            match resolved.map(|d| d.handler) {
                Some(FunctionId::Date(function)) => Expression::Date(DateExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Logical(function)) => Expression::Logical(LogicalExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Statistics(function)) => Expression::Statistics(StatisticsExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Reference(function)) => Expression::Reference(ReferenceExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Search(function)) => Expression::Search(SearchExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Information(function)) => {
                    Expression::Information(InformationExpr {
                        function,
                        arguments,
                    })
                }
                Some(FunctionId::Math(function)) => Expression::Math(MathExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Text(function)) => Expression::Text(TextExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Financial(function)) => Expression::Financial(FinancialExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Engineering(function)) => {
                    Expression::Engineering(EngineeringExpr {
                        function,
                        arguments,
                    })
                }
                Some(FunctionId::Matrix(function)) => Expression::Matrix(MatrixExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Criteria(function)) => Expression::Criteria(CriteriaExpr {
                    function,
                    arguments,
                }),
                Some(FunctionId::Database(function)) => Expression::Database(DatabaseExpr {
                    function,
                    arguments,
                }),
                None => match registry.and_then(|registry| registry.resolve(lookup_name)) {
                    Some(binding) => Expression::Extension { binding, arguments },
                    None => Expression::UnresolvedFunction(arguments),
                },
            }
        }
    }
}
