#![doc = include_str!("../../FORMULA.md")]

mod ast;
/// Allocation-light token classification used for safe compiler-template rebinding.
pub mod classified;
mod function_name;
/// Interned builtin symbols and owned extension spellings used during binding.
pub use function_name::{CommonFunction, FunctionName};
mod metadata;
mod parser;
mod reference_classes;
mod references;
mod syntax;
mod tokens;
mod translation;

/// Workbook-independent syntax, reference coordinates, and parse diagnostics.
pub use ast::{BinaryOp, CellReference, Expr, ParseError, SheetSpanReference, UnaryOp};
/// Parsed source with explicit limits and spans for diagnostics and rewriting.
pub use metadata::{FormulaParsePolicy, FormulaSpan, FunctionInvocation, ParsedFormula};
/// Full formula parsing with default or explicitly bounded grammar policy.
pub use parser::{parse_formula, parse_formula_with_policy};
/// Precedence-aware source printing used by edits and file adapters.
pub use syntax::{FormulaPrinter, GroupingContext, needs_parentheses};
/// Checked relative-reference translation before source replacement.
pub use translation::{
    FormulaLocation, TranslationBoundaryPolicy, TranslationPlan, TranslationPolicy,
};

#[cfg(test)]
mod tests;
