//! Parse formulas into expressions, print them, and translate cell references.
//!
//! The expression tree retains reference spellings without resolving workbook
//! names. `classified` is a separate fast path for reusing compiled templates;
//! it does not validate formula syntax.

mod ast;
mod classification_error;
/// Allocation-light token classification used for safe compiler-template rebinding.
pub mod classified;
mod function_name;
mod let_form;
mod metadata;
mod parse_error;
mod parse_error_kind;
mod parse_reason_error;
mod parser;
mod reference_classes;
mod references;
mod syntax;
mod table_columns;
mod table_reference;
mod table_section;
mod tokens;
mod translation;

pub use ast::{BinaryOp, CellReference, Expr, SheetSpanReference, UnaryOp};
// Interned builtin symbols and owned extension spellings used during binding.
pub use function_name::{CommonFunction, FunctionName};
pub use let_form::LetForm;
// Parsed source with explicit limits and spans for diagnostics and rewriting.
pub use metadata::{FormulaParsePolicy, FormulaSpan, FunctionInvocation, ParsedFormula};
pub use parser::{parse_formula, parse_formula_with_policy};
pub use syntax::{FormulaPrinter, GroupingContext, needs_parentheses};
pub use table_columns::TableColumns;
pub use table_reference::TableReference;
pub use table_section::TableSection;
// Checked relative-reference translation before source replacement.
pub use translation::{
    FormulaLocation, TranslationBoundaryPolicy, TranslationPlan, TranslationPolicy,
};

#[cfg(test)]
mod tests;

pub use parse_error::ParseError;
pub use parse_error_kind::ParseErrorKind;
pub use parse_reason_error::ParseReasonError;
