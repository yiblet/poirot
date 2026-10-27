//! Operation contracts kept beside inferred facts, without adding tags to runtime values.
use super::argument_evaluation::ArgumentEvaluation;
use super::conditional_arguments::ConditionalArguments;
use crate::eval::{
    compile::bytecode::CallTarget,
    functions::registry::{EvaluationStrategy, FunctionId, LogicalFunction, ResultRule},
};
use poirot_parsing::formula::{BinaryOp, Expr, FunctionName, UnaryOp};

/// Transfer rule for an expression's forms, element kinds, dimensions and operand demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExpressionContract {
    /// Introduce a literal or unresolved name without elementwise lifting.
    Atom,
    /// Broadcast scalar/reference/array operands along rows and columns.
    Broadcast,
    /// Select a scalar from an operand through explicit implicit intersection.
    ScalarIntersection,
    /// Construct or select a reference, retaining its coordinate-derived bounds.
    Reference,
    /// Construct an array from its scalar element expressions.
    Array,
    /// Apply a registry transfer rule and preserve its operand-demand protocol.
    Function {
        /// Scalar, reduction, broadcasting, selection or array-size transfer rule.
        result: ResultRule,
        /// Which arguments may be requested, without executing any of them during inference.
        arguments: ArgumentEvaluation,
    },
}

impl ExpressionContract {
    /// Describe a syntax operator before any child expression is evaluated.
    pub(super) fn of(expression: &Expr) -> Self {
        match expression {
            Expr::Unary {
                op: UnaryOp::ImplicitIntersection,
                ..
            } => Self::ScalarIntersection,
            Expr::Unary {
                op: UnaryOp::SpillRange,
                ..
            }
            | Expr::Cell(_)
            | Expr::Column { .. }
            | Expr::Row { .. }
            | Expr::SheetSpan(_) => Self::Reference,
            Expr::Binary {
                op: BinaryOp::Range | BinaryOp::Union | BinaryOp::Intersection,
                ..
            } => Self::Reference,
            Expr::Binary { .. } | Expr::Unary { .. } => Self::Broadcast,
            Expr::Array(_) => Self::Array,
            Expr::Function { .. } => Self::Function {
                result: ResultRule::Any,
                arguments: ArgumentEvaluation::HandlerControlled,
            },
            _ => Self::Atom,
        }
    }

    /// Use the same resolved target as bytecode; unknown/provider calls stay conservative.
    pub(super) fn function(name: &FunctionName, target: &CallTarget) -> Self {
        let handler = match target {
            CallTarget::Builtin(handler) => Some(*handler),
            CallTarget::Unresolved => {
                crate::eval::functions::registry::resolve_excel_function(name)
            }
            CallTarget::Extension(_) => None,
        };
        let Some(handler) = handler else {
            return Self::Function {
                result: ResultRule::Any,
                arguments: ArgumentEvaluation::HandlerControlled,
            };
        };
        let descriptor = handler.descriptor();
        let selected = match handler {
            FunctionId::Logical(LogicalFunction::If) => Some(ConditionalArguments::If),
            FunctionId::Logical(LogicalFunction::IfError) => Some(ConditionalArguments::IfError),
            FunctionId::Logical(LogicalFunction::IfNa) => Some(ConditionalArguments::IfNa),
            FunctionId::Logical(LogicalFunction::Choose) => Some(ConditionalArguments::Choose),
            FunctionId::Logical(LogicalFunction::Ifs) => Some(ConditionalArguments::Ifs),
            FunctionId::Logical(LogicalFunction::Switch) => Some(ConditionalArguments::Switch),
            _ => None,
        };
        let arguments =
            selected
                .map(ArgumentEvaluation::Conditional)
                .unwrap_or(match descriptor.strategy {
                    EvaluationStrategy::EagerErrorsBeforeArity => {
                        ArgumentEvaluation::EagerErrorsBeforeArity
                    }
                    EvaluationStrategy::Lazy | EvaluationStrategy::FamilyControlled => {
                        ArgumentEvaluation::HandlerControlled
                    }
                });
        Self::Function {
            result: descriptor.result,
            arguments,
        }
    }
}
