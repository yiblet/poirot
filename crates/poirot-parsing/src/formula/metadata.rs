//! Immutable parse limits and retained source metadata.
use crate::formula::Expr;

/// Limits on formula size, nesting, and reference coordinates.
///
/// Each limit must be nonzero and no larger than its default value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaParsePolicy {
    /// Maximum number of expression-parsing steps for one formula.
    pub max_expressions: usize,
    /// Maximum expression nesting accepted by the parser.
    pub max_depth: usize,
    /// Exclusive row-coordinate bound for accepted references.
    pub max_rows: u32,
    /// Exclusive column-coordinate bound for accepted references.
    pub max_columns: u32,
}
impl Default for FormulaParsePolicy {
    fn default() -> Self {
        Self {
            max_expressions: 1024,
            max_depth: 128,
            max_rows: 1048576,
            max_columns: 16384,
        }
    }
}
/// Observed expression invocation range, in UTF-8 bytes of the original source.
/// Ranges can overlap and do not identify individual semantic AST nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormulaSpan {
    /// Inclusive UTF-8 byte offset in the original source.
    pub start: usize,
    /// Exclusive UTF-8 byte offset in the original source.
    pub end: usize,
}
/// The source ranges of a parsed function name and its complete call.
///
/// Offsets are UTF-8 bytes in the original formula text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionInvocation {
    /// Source span of the function spelling.
    pub name_span: FormulaSpan,
    /// Source span of the complete call expression.
    pub expression_span: FormulaSpan,
    /// Number of argument slots, including omitted arguments.
    pub argument_count: usize,
}
/// A parsed formula with its original text and source spans.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFormula {
    /// Original formula source used by spans and diagnostics.
    pub(super) source: std::sync::Arc<str>,
    /// Parsed syntax consumed by binding and translation.
    pub(super) expression: Expr,
    /// Source locations associated with parsed expressions.
    pub(super) spans: Vec<FormulaSpan>,
    /// Call-site metadata for inspection without walking the AST.
    pub(super) function_invocations: Vec<FunctionInvocation>,
    /// Parser limits under which this source was accepted.
    pub(super) policy: FormulaParsePolicy,
}
impl ParsedFormula {
    /// Borrow the original source; span offsets refer to these bytes.
    pub fn source(&self) -> &str {
        &self.source
    }
    /// Borrow the AST for binding, analysis, or translation.
    pub fn expression(&self) -> &Expr {
        &self.expression
    }
    /// Borrow the recorded source spans.
    pub fn expression_spans(&self) -> &[FormulaSpan] {
        &self.spans
    }
    /// Actual function nodes in source order; repeated invocations remain separate.
    pub fn function_invocations(&self) -> &[FunctionInvocation] {
        &self.function_invocations
    }
    /// Return the limits used when parsing this formula.
    pub fn policy(&self) -> FormulaParsePolicy {
        self.policy
    }
    /// Consume the parsed wrapper and retain only its AST.
    pub fn into_expression(self) -> Expr {
        self.expression
    }
}
