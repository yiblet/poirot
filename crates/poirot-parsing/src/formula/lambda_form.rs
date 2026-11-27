use super::ast::Expr;
use super::let_form::{LetForm, valid_name};
use super::table_columns::TableColumns;
use super::table_section::TableSection;

/// Borrow the parameter declarations and deferred body of a LAMBDA expression.
pub struct LambdaForm<'a> {
    parameters: &'a [Expr],
    /// Formula to evaluate when the callable is invoked.
    pub body: &'a Expr,
}

impl<'a> LambdaForm<'a> {
    /// Whether the function spelling denotes LAMBDA, including its XLSX prefix.
    pub fn matches(function: &str) -> bool {
        function
            .strip_prefix("_xlfn.")
            .unwrap_or(function)
            .eq_ignore_ascii_case("LAMBDA")
    }

    /// Validate positional parameter declarations and a final body expression.
    pub fn parse(arguments: &'a [Expr]) -> Option<Self> {
        let (body, parameters) = arguments.split_last()?;
        if parameters.len() > 253 || matches!(body, Expr::Missing) {
            return None;
        }
        for (index, parameter) in parameters.iter().enumerate() {
            let name = parameter_name(parameter)?;
            // The documented MAKEARRAY example uses bare r and c as Lambda parameters.
            if !valid_name(name)
                || name.contains('.')
                || parameters
                    .iter()
                    .take(index)
                    .filter_map(parameter_name)
                    .any(|earlier| earlier.eq_ignore_ascii_case(name))
            {
                return None;
            }
        }
        Some(Self { parameters, body })
    }

    /// Number of local parameters accepted by the callable.
    pub fn arity(&self) -> usize {
        self.parameters.len()
    }

    /// Minimum number of positional slots needed to include every required parameter.
    pub fn minimum_arity(&self) -> usize {
        self.parameters
            .iter()
            .rposition(|parameter| !optional_declaration(parameter))
            .map_or(0, |index| index.saturating_add(1))
    }

    /// Borrow the normalized parameter spellings in declaration order.
    pub fn parameters(&self) -> impl Iterator<Item = &'a str> {
        self.parameters.iter().filter_map(parameter_name)
    }
}

/// Read the identifier in a parameter position.
fn parameter_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => {
            Some(stored_optional_name(name).unwrap_or_else(|| LetForm::local_name(name)))
        }
        Expr::Table(table) if table.table.is_none() && table.section == TableSection::Data => {
            match &table.columns {
                TableColumns::One(name) => Some(name),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Detect an optional parameter declaration in formula or stored notation.
fn optional_declaration(expression: &Expr) -> bool {
    match expression {
        Expr::Table(_) => true,
        Expr::Name(name) => stored_optional_name(name).is_some(),
        _ => false,
    }
}

/// Return a stored optional parameter's unprefixed name.
fn stored_optional_name(name: &str) -> Option<&str> {
    let (prefix, local) = name.split_once('.')?;
    prefix.eq_ignore_ascii_case("_xlop").then_some(local)
}
