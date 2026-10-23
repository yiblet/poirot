//! Immutable formula syntax and parse errors.
use std::fmt;

/// Formula syntax before workbook name resolution or evaluation.
///
/// Reference coordinates are zero-based. Names remain spellings, and an omitted
/// argument (`Missing`) is distinct from an empty string or a blank worksheet cell.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Numeric literal before the numeric policy is applied.
    Number(f64),
    /// Quoted text literal with decoded escaping.
    String(String),
    /// Boolean literal.
    Bool(bool),
    /// Spreadsheet error spelling retained for binding.
    Error(String),
    /// Omitted function argument, distinct from an explicit blank.
    Missing,
    /// A single address resolved against the evaluation workbook.
    Cell(CellReference),
    /// A rectangle repeated across ordered sheet endpoints.
    SheetSpan(SheetSpanReference),
    /// Whole-column reference; row extent comes from worksheet limits.
    Column {
        /// Zero-based column coordinate.
        column: u32,
        /// Preserve this axis when translating the formula.
        absolute: bool,
        /// Optional explicit sheet name; otherwise use the current sheet.
        sheet: Option<String>,
    },
    /// Whole-row reference; column extent comes from worksheet limits.
    Row {
        /// Zero-based row coordinate.
        row: u32,
        /// Preserve this axis when translating the formula.
        absolute: bool,
        /// Optional explicit sheet name; otherwise use the current sheet.
        sheet: Option<String>,
    },
    /// Unqualified defined name resolved by workbook and sheet scope.
    Name(String),
    /// A defined name looked up in an explicit worksheet scope.
    QualifiedName {
        /// Sheet in which to look up the name.
        sheet: String,
        /// Name spelling retained until binding or workbook resolution.
        name: String,
    },
    /// Prefix or postfix operation on one expression.
    Unary {
        /// Operation applied by the evaluator.
        op: UnaryOp,
        /// Expression consumed by the unary operation.
        operand: Box<Expr>,
    },
    /// Value or reference operation combining two expressions.
    Binary {
        /// Operation applied by the evaluator.
        op: BinaryOp,
        /// Left operand, evaluated before the right operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Unbound call spelling and ordered argument expressions.
    Function {
        /// Name spelling retained until binding or workbook resolution.
        name: crate::formula::FunctionName,
        /// Arguments in source order, including omitted slots.
        args: Vec<Expr>,
    },
    /// Row-major array constant; the parser checks rectangularity.
    Array(Vec<Vec<Expr>>),
}
/// An A1-style reference with zero-based coordinates and optional sheet name.
/// Absolute flags preserve `$` anchoring for copy translation; they do not change
/// the coordinate read during ordinary evaluation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CellReference {
    /// Zero-based row coordinate.
    pub row: u32,
    /// Zero-based column coordinate.
    pub column: u32,
    /// Preserve this row when copying the formula.
    pub row_absolute: bool,
    /// Preserve this column when copying the formula.
    pub column_absolute: bool,
    /// Optional explicit sheet name; otherwise use the current sheet.
    pub sheet: Option<String>,
}
/// A cell or rectangle on each sheet between two named sheets, inclusive.
///
/// The parser retains sheet names; the evaluator resolves their workbook order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SheetSpanReference {
    /// First sheet endpoint, resolved by workbook order.
    pub first_sheet: String,
    /// Last sheet endpoint, inclusive.
    pub last_sheet: String,
    /// First cell endpoint of the referenced rectangle.
    pub start: CellReference,
    /// Optional opposite endpoint; absence denotes a single cell.
    pub end: Option<CellReference>,
}
/// Prefix or postfix formula operator, including percent and explicit intersection (`@`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Unary plus with the selected numeric coercion policy.
    Plus,
    /// Negate the coerced numeric operand.
    Minus,
    /// Divide the coerced numeric operand by one hundred.
    Percent,
    /// Explicit `@` selection without numeric coercion.
    ImplicitIntersection,
    /// Reference the accepted spill footprint of a single anchor (`#`).
    SpillRange,
}
/// Arithmetic, comparison, text, or reference-combination operator in the syntax tree.
/// Range (`:`), union (`,`), and intersection (space) operate on references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Add numeric operands.
    Add,
    /// Subtract the right numeric operand from the left.
    Subtract,
    /// Multiply numeric operands.
    Multiply,
    /// Divide the left numeric operand by the right.
    Divide,
    /// Raise the left numeric operand to the right power.
    Power,
    /// Concatenate operands using text coercion.
    Concat,
    /// Test operand equality.
    Equal,
    /// Test operand inequality.
    NotEqual,
    /// Test whether the left operand sorts before the right.
    Less,
    /// Test less-than or equality.
    LessEqual,
    /// Test whether the left operand sorts after the right.
    Greater,
    /// Test greater-than or equality.
    GreaterEqual,
    /// Construct the rectangle spanning two reference endpoints.
    Range,
    /// Combine reference areas without scalar coercion.
    Union,
    /// Select the overlap of two reference areas.
    Intersection,
}
/// Why formula preparation rejected source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParseErrorKind {
    /// Source text is outside the supported formula grammar.
    Syntax,
    /// A configured structural bound rejected otherwise parseable source.
    ResourceLimit,
    /// A parser limit is zero or exceeds its supported maximum.
    InvalidPolicy,
}
/// Formula syntax or parser-limit failure at a UTF-8 byte offset in the input.
/// This is a parsing diagnostic, not a spreadsheet cell error value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ParseError {
    /// Whether rejection concerns syntax, resource capacity, or configuration.
    pub kind: ParseErrorKind,
    /// UTF-8 byte offset where parsing failed.
    pub position: usize,
    /// Diagnostic explaining the rejected syntax or parser limit.
    pub message: String,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.position)
    }
}
impl std::error::Error for ParseError {}
