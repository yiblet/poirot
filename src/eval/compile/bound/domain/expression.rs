use super::arguments::Arguments;
use super::criteria_expr::CriteriaExpr;
use super::database_expr::DatabaseExpr;
use super::date_expr::DateExpr;
use super::engineering_expr::EngineeringExpr;
use super::financial_expr::FinancialExpr;
use super::information_expr::InformationExpr;
use super::logical_expr::LogicalExpr;
use super::math_expr::MathExpr;
use super::matrix_expr::MatrixExpr;
use super::reference_expr::ReferenceExpr;
use super::search_expr::SearchExpr;
use super::statistics_expr::StatisticsExpr;
use super::text_expr::TextExpr;
use poirot_parsing::formula::{BinaryOp, CellReference, UnaryOp};

/// Executable formula tree with resolved builtin/extension handler identities.
/// Workbook references and names remain deferred, so lazy branches are evaluated only
/// when selected. [`FormulaPlan`](crate::eval::compile::bound::FormulaPlan) retains the
/// original syntax separately.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    /// Numeric literal.
    Number(f64),
    /// Text literal.
    String(String),
    /// Boolean literal.
    Bool(bool),
    /// Spreadsheet error spelling.
    Error(String),
    /// Omitted argument, distinct from a blank cell.
    Missing,
    /// Deferred single-cell reference.
    Cell(CellReference),
    /// Deferred reference spanning multiple sheets.
    SheetSpan(poirot_parsing::formula::SheetSpanReference),
    /// Whole-column reference.
    Column {
        /// Zero-based column index.
        column: u32,
        /// Whether the column is absolute when translated.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
    /// Whole-row reference.
    Row {
        /// Zero-based row index.
        row: u32,
        /// Whether the row is absolute when translated.
        absolute: bool,
        /// Optional sheet qualifier.
        sheet: Option<String>,
    },
    /// Unqualified defined-name lookup.
    Name(String),
    /// Sheet-qualified defined-name lookup.
    QualifiedName {
        /// Qualifying sheet spelling.
        sheet: String,
        /// Defined-name spelling.
        name: String,
    },
    /// Unary operation over a deferred operand.
    Unary {
        /// Unary operator to execute.
        op: UnaryOp,
        /// Deferred operand.
        operand: Box<Expression>,
    },
    /// Binary value or reference operation.
    Binary {
        /// Binary operator to execute.
        op: BinaryOp,
        /// Deferred left operand.
        left: Box<Expression>,
        /// Deferred right operand.
        right: Box<Expression>,
    },
    /// Row-major array of deferred element expressions.
    Array(Vec<Vec<Expression>>),
    /// Financial builtin call.
    Financial(FinancialExpr),
    /// Engineering builtin call.
    Engineering(EngineeringExpr),
    /// Matrix builtin call.
    Matrix(MatrixExpr),
    /// Criteria aggregation call.
    Criteria(CriteriaExpr),
    /// Database builtin call.
    Database(DatabaseExpr),
    /// Date or time builtin call.
    Date(DateExpr),
    /// Logical builtin call.
    Logical(LogicalExpr),
    /// Statistical builtin call.
    Statistics(StatisticsExpr),
    /// Reference builtin call.
    Reference(ReferenceExpr),
    /// Lookup builtin call.
    Search(SearchExpr),
    /// Value-inspection builtin call.
    Information(InformationExpr),
    /// Mathematical builtin call.
    Math(MathExpr),
    /// Text builtin call.
    Text(TextExpr),
    /// Extension call bound to an explicit registry entry.
    Extension {
        /// Exact extension registration identity.
        binding: crate::eval::functions::extensions::ExtensionBinding,
        /// Deferred operands and spelling metadata.
        arguments: Arguments,
    },
    /// Error is deferred until this expression is actually selected/evaluated.
    UnresolvedFunction(Arguments),
}
