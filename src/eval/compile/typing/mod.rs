//! Possible-result abstract interpretation at the existing bytecode expression boundaries.
//!
//! Inference tracks unions of scalar kinds, arrays and references, together with
//! conservative row/column bounds and possible effects. Compilation advances this
//! state alongside bytecode emission. Unknown names and function contracts remain
//! conservative; inferred bounds never establish runtime spill ownership.
//! Actual consumed dependencies determine whether evaluated values can be reused.
//!
//! Depends on: functions (registry identities); sibling bytecode (expression boundaries).

/// Operand-demand contract of a function call.
mod argument_evaluation;
/// Selection protocol whose operands remain conditional during execution.
mod conditional_arguments;
/// Transfer rule for an expression's forms, element kinds, dimensions and operand demand.
mod expression_contract;
/// Inferred possible values, maximum array dimensions, and effects for an expression.
mod expression_properties;
/// Possible external behavior, ordered Tracked <= Volatile <= Opaque.
mod external_behavior;
/// Conservative bounds on the workbook services and external behavior an expression may use.
mod formula_effects;
/// Inference results for every expression in a prepared executable.
mod formula_typing;
/// One suspended expression in the shared depth-first preparation traversal.
mod inference_frame;
/// Abstract interpreter advanced by the same enter/bind/leave events as bytecode emission.
mod inference_machine;
/// Unit tests for expression typing and prepared formula properties.
#[cfg(test)]
mod tests;
/// Property transfer rules applied while inferring expression types.
mod transfer;
/// Connects an expression's inferred properties and evaluation contract to its bytecode entry.
mod typed_expression;

pub use argument_evaluation::ArgumentEvaluation;
pub use conditional_arguments::ConditionalArguments;
pub use expression_contract::ExpressionContract;
pub use expression_properties::ExpressionProperties;
pub use external_behavior::ExternalBehavior;
pub use formula_effects::FormulaEffects;
pub use formula_typing::FormulaTyping;
pub(in crate::eval) use inference_machine::InferenceMachine;
pub use typed_expression::TypedExpression;
