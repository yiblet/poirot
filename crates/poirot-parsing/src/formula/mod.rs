//! Parse formulas into expressions, print them, and translate cell references.
//!
//! The expression tree retains reference spellings without resolving workbook
//! names. `classified` is a separate fast path for reusing compiled templates;
//! it does not validate formula syntax.

mod ast;
/// Allocation-light token classification used for safe compiler-template rebinding.
pub mod classified;
mod function_name;
// Interned builtin symbols and owned extension spellings used during binding.
pub use function_name::{CommonFunction, FunctionName};
mod metadata;
mod parser;
mod reference_classes;
mod references;
mod syntax;
mod tokens;
mod translation;

pub use ast::{
    BinaryOp, CellReference, Expr, ParseError, ParseErrorKind, SheetSpanReference, UnaryOp,
};
// Parsed source with explicit limits and spans for diagnostics and rewriting.
pub use metadata::{FormulaParsePolicy, FormulaSpan, FunctionInvocation, ParsedFormula};
pub use parser::{parse_formula, parse_formula_with_policy};
pub use syntax::{FormulaPrinter, GroupingContext, needs_parentheses};
// Checked relative-reference translation before source replacement.
pub use translation::{
    FormulaLocation, TranslationBoundaryPolicy, TranslationPlan, TranslationPolicy,
};

#[cfg(test)]
mod tests;
