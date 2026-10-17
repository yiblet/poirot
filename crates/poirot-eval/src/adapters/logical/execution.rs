//! Logical family owns lazy selection and argument-sensitive evaluation order.
use crate::*;
impl Session<'_> {
    /// Execute logical tests and selected branches, preserving operational failures.
    pub(crate) fn logical_function(
        &mut self,
        s: usize,
        at: CellAddress,
        function: LogicalFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        if let Some(function) = function.helper() {
            return adapters::logical::execute(self, s, at, function, args);
        }
        use LogicalFunction::*;
        let arity = FunctionId::Logical(function).descriptor().arity;
        Ok(match function {
            IsRef | ErrorType | Ifs | IsEven | IsOdd | Switch => return Ok(err(VALUE_ERROR)),
            If => {
                if !arity.accepts(args.len()) {
                    return Ok(err(VALUE_ERROR));
                }
                let condition = self.argument(s, at, args, 0)?;
                match core_projection::coercev2_operand_boolean(&condition, self.numeric_policy) {
                    Err(e) => err(e),
                    Ok(condition) => match args.get(if condition { 1 } else { 2 }) {
                        Some(expression) if expression.is_missing() => Blank.into(),
                        Some(expression) => {
                            self.observed_value_argument(
                                s,
                                at,
                                expression,
                                if condition { 1 } else { 2 },
                            )?
                            .1
                        }
                        None => Boolean(false).into(),
                    },
                }
            }
            IfError | IfNa => {
                if !arity.accepts(args.len()) {
                    return Ok(err(VALUE_ERROR));
                }
                let (source, value) = self.observed_value_argument(
                    s,
                    at,
                    args.get(0).ok_or(EvaluationFailure::StalePlan)?,
                    0,
                )?;
                let check = self.scalar(at, value.clone())?;
                let caught = match check {
                    Error(error) if function == IfError || error == NA => Some(error),
                    _ => None,
                };
                if caught.is_some() {
                    self.observed_fallback(
                        s,
                        at,
                        args.get(1).ok_or(EvaluationFailure::StalePlan)?,
                        function,
                        source,
                        &check,
                    )?
                } else {
                    self.observe_mask(function, source, &check, caught, None)?;
                    value
                }
            }
            Choose => {
                if !arity.accepts(args.len()) {
                    return Ok(err(VALUE_ERROR));
                }
                let value = self.argument(s, at, args, 0)?;
                match semantic(self.coercev2_scalar_number(&value))? {
                    Ok(index) if index >= 1. && index.trunc() < (args.len() as f64) => {
                        self.observed_value_argument(
                            s,
                            at,
                            args.get(index as usize)
                                .ok_or(EvaluationFailure::StalePlan)?,
                            index as usize,
                        )?
                        .1
                    }
                    Ok(_) => err(VALUE_ERROR),
                    Err(e) => err(e),
                }
            }
            And => self.aggregate(s, at, AggregateFunction::And, args)?,
            Or => self.aggregate(s, at, AggregateFunction::Or, args)?,
            True | False => {
                if arity.accepts(args.len()) {
                    Boolean(function == True).into()
                } else {
                    err(VALUE_ERROR)
                }
            }
            Not => match self.eager_arguments(s, at, args)? {
                Err(e) => err(e),
                Ok(values) if arity.accepts(values.len()) => {
                    match core_projection::coercev2_operand_boolean(
                        values.first().ok_or(EvaluationFailure::StalePlan)?,
                        self.numeric_policy,
                    ) {
                        Ok(value) => Boolean(!value).into(),
                        Err(e) => err(e),
                    }
                }
                Ok(_) => err(VALUE_ERROR),
            },
        })
    }
}
