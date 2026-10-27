use super::call_context::CallContext;
use super::function_value::FunctionValue;
use crate::eval::limits::EvaluationError;
use crate::eval::values::Value;
use crate::eval::vm::{ExecutableRef, ExpressionRunner};
use poirot_functions::scalar::Input;

/// A deferred expression memoized after successful evaluation. Repeated forces copy that value
/// under the host budget, without repeating volatile operations.
pub struct FunctionInput<'input, 'call, 'workbook> {
    pub(super) context: &'input mut CallContext<'call, 'workbook>,
    pub(super) expression: ExecutableRef<'input>,
    pub(super) value: Option<Value>,
}

impl Input for FunctionInput<'_, '_, '_> {
    /// Adapter argument value returned by deferred evaluation.
    type Value = FunctionValue;

    /// Operational failure propagated when forcing the argument.
    type Error = EvaluationError;

    fn force(&mut self) -> Result<FunctionValue, EvaluationError> {
        if self.value.is_none() {
            let target = self.context.target;
            self.context.session.state.runtime.budget.allocation(1)?;
            let value = ExpressionRunner::new(self.context.session).expr(
                target.sheet,
                target.address,
                self.expression,
            )?;
            self.value = Some(if self.expression.is_missing() {
                Value::Missing
            } else {
                value
            });
        }
        crate::eval::values::copy_value(
            &mut self.context.session.state.runtime.budget,
            // SAFETY: If value is None this function assigns Some before reaching this access; no intervening code clears it.
            self.value.as_ref().unwrap(),
        )
        .map(FunctionValue)
    }
}
