use super::ast::Expr;
use super::references::cell_reference;

/// Borrow the declarations and final expression of a syntactically valid LET call.
pub struct LetForm<'a> {
    pairs: &'a [[Expr; 2]],
    /// Final expression evaluated after all declarations.
    pub result: &'a Expr,
}

impl<'a> LetForm<'a> {
    /// Whether this spelling identifies the worksheet LET binding form.
    pub fn matches(function: &str) -> bool {
        function
            .strip_prefix("_xlfn.")
            .unwrap_or(function)
            .eq_ignore_ascii_case("LET")
    }

    /// Validate the alternating declarations and single final expression.
    pub fn parse(arguments: &'a [Expr]) -> Option<Self> {
        let (result, pairs) = arguments.split_last()?;
        let (pairs, remainder) = pairs.as_chunks::<2>();
        if !(1..=126).contains(&pairs.len())
            || !remainder.is_empty()
            || matches!(result, Expr::Missing)
            || pairs.iter().any(|[name, _]| match name {
                Expr::Name(name) => !valid_name(Self::local_name(name)),
                _ => true,
            })
        {
            return None;
        }
        Some(Self { pairs, result })
    }

    /// Iterate declaration spellings and initializers in source order.
    pub fn bindings(&self) -> impl Iterator<Item = (&'a str, &'a Expr)> {
        self.pairs.iter().filter_map(|pair| match pair {
            [Expr::Name(name), value] => Some((Self::local_name(name), value)),
            _ => None,
        })
    }

    /// Remove the stored local-parameter prefix from a lexical identifier.
    pub fn local_name(text: &str) -> &str {
        text.strip_prefix("_xlpm.").unwrap_or(text)
    }
}

/// Check a local name without admitting A1 and R1C1 reference spellings.
fn valid_name(name: &str) -> bool {
    if name.eq_ignore_ascii_case("R") || name.eq_ignore_ascii_case("C") {
        return false;
    }
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '\\'))
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
        && matches!(cell_reference(name, None), Ok(None))
        && !name
            .strip_prefix(['R', 'r'])
            .and_then(|suffix| suffix.split_once(['C', 'c']))
            .is_some_and(|(row, column)| digits(row) && digits(column))
}

/// Whether a reference coordinate is a nonempty ASCII digit sequence.
fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}
