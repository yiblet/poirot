//! Immutable executable domain records and their read-only accessors.
use crate::registry::*;
use poirot_parsing::formula::{BinaryOp, CellReference, Expr as Syntax, UnaryOp};
/// Deferred operands of a bound function call with source and lookup spellings.
/// Binding selects a handler but does not evaluate these expressions.
#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    /// Function spelling retained from parsed syntax.
    pub source_name: String,
    /// Exact lookup spelling when it differs from the parsed AST name.
    pub resolved_name: Option<String>,
    /// Deferred argument expressions in source order.
    pub expressions: Vec<Expression>,
}
impl Arguments {
    /// Construct manually bound arguments with the parsed spelling as lookup spelling.
    /// Manual inputs do not establish compiled-source evidence or backend parity.
    pub fn new(source_name: impl Into<String>, expressions: Vec<Expression>) -> Self {
        Self {
            source_name: source_name.into(),
            resolved_name: None,
            expressions,
        }
    }
    /// Retain exact binding lookup metadata; equal names need no second allocation.
    pub fn with_resolved_spelling(mut self, spelling: &str) -> Self {
        self.resolved_name = (spelling != self.source_name).then(|| spelling.to_owned());
        self
    }
    /// Return the exact spelling used for handler lookup.
    pub fn resolved_spelling(&self) -> &str {
        self.resolved_name.as_deref().unwrap_or(&self.source_name)
    }
}
/// Bound date or time call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct DateExpr {
    /// Selected date handler.
    pub function: DateFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound logical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct LogicalExpr {
    /// Selected logical handler.
    pub function: LogicalFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound statistical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct StatisticsExpr {
    /// Selected statistical handler.
    pub function: StatisticsFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound reference call, preserving deferred reference operands.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceExpr {
    /// Selected reference handler.
    pub function: ReferenceFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound lookup call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchExpr {
    /// Selected lookup handler.
    pub function: SearchFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound value-inspection call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct InformationExpr {
    /// Selected information handler.
    pub function: InformationFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound mathematical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct MathExpr {
    /// Selected math handler.
    pub function: MathFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound text call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct TextExpr {
    /// Selected text handler.
    pub function: TextFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound financial call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct FinancialExpr {
    /// Selected financial handler.
    pub function: FinancialFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound engineering call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringExpr {
    /// Selected engineering handler.
    pub function: EngineeringFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound matrix call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixExpr {
    /// Selected matrix handler.
    pub function: MatrixFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound criteria aggregation call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct CriteriaExpr {
    /// Selected criteria handler.
    pub function: CriteriaFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Bound database call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct DatabaseExpr {
    /// Selected database handler.
    pub function: DatabaseFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
/// Executable formula tree with resolved builtin/extension handler identities.
/// Workbook references and names remain deferred, so lazy branches are evaluated only
/// when selected. [`FormulaPlan`] retains the original syntax separately.
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
        binding: crate::extensions::ExtensionBinding,
        /// Deferred operands and spelling metadata.
        arguments: Arguments,
    },
    /// Error is deferred until this expression is actually selected/evaluated.
    UnresolvedFunction(Arguments),
}
/// Caller-owned syntax plus immutable bytecode; bound inspection views are constructed on demand.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaPlan {
    /// Original syntax retained for caller-requested inspection.
    pub(super) syntax: Syntax,
    /// The sole production execution representation.
    pub(super) bytecode: crate::bytecode::Compiled,
    /// Registry identity required by bound extension calls.
    pub(super) extension_identity: Option<crate::extensions::ExtensionIdentity>,
}
impl FormulaPlan {
    /// Borrow the immutable bytecode used by production execution.
    pub fn bytecode(&self) -> &crate::bytecode::Compiled {
        &self.bytecode
    }
    /// Return the extension registry identity captured during binding.
    pub fn extension_identity(&self) -> Option<&crate::extensions::ExtensionIdentity> {
        self.extension_identity.as_ref()
    }
    /// Borrow the original parsed syntax.
    pub fn syntax(&self) -> &Syntax {
        &self.syntax
    }
    /// Construct an owned handler-bound inspection view without retaining an execution tree.
    pub fn expression(&self) -> Expression {
        super::bind_with_executable(&self.syntax, &self.bytecode.executable)
    }
}
