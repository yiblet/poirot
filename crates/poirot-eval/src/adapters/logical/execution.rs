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
                self.read(DependencyRead::BuiltinPolicy);
                let condition = if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation
                {
                    let (_, value) = self.observed_value_argument(
                        s,
                        at,
                        args.get(0).ok_or(EvaluationFailure::StalePlan)?,
                        0,
                    )?;
                    if self.retains_array(&value) {
                        return self.array_if(s, at, args, value);
                    }
                    self.scalar(at, value)?
                } else {
                    self.argument(s, at, args, 0)?
                };
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
                self.read(DependencyRead::BuiltinPolicy);
                let value = if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
                    let (_, value) = self.observed_value_argument(
                        s,
                        at,
                        args.get(0).ok_or(EvaluationFailure::StalePlan)?,
                        0,
                    )?;
                    if self.retains_array(&value) {
                        return self.array_choose(s, at, args, value);
                    }
                    self.scalar(at, value)?
                } else {
                    self.argument(s, at, args, 0)?
                };
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

impl Session<'_> {
    /// Select corresponding members, broadcasting singleton condition and branch axes.
    fn array_if(
        &mut self,
        sheet: usize,
        at: CellAddress,
        args: ArgumentsRef<'_>,
        condition: Value,
    ) -> EvalResult<Value> {
        use crate::array_operator::Shape;
        let condition_shape = Shape::inspect(&condition).ok_or_else(|| {
            self.spill_unsupported(sheet, at, "IF requires one rectangular condition")
        })?;
        let mut needs_true = false;
        let mut needs_false = false;
        let condition = self.rectangular_values(at, condition_shape, |session, row, column| {
            let value = session.rectangular_element(&condition, condition_shape, row, column)?;
            Ok(
                match core_projection::coercev2_operand_boolean(&value, session.numeric_policy) {
                    Ok(value) => {
                        needs_true |= value;
                        needs_false |= !value;
                        Boolean(value)
                    }
                    Err(error) => Error(error),
                },
            )
        })?;
        // Only requested branches are evaluated, once per invocation. References remain
        // deferred so skipped member reads do not create dependencies or operational errors.
        let mut branch = |ordinal: usize, needed: bool| -> EvalResult<Option<(Value, Shape)>> {
            if !needed {
                return Ok(None);
            }
            let value = match args.get(ordinal) {
                Some(expression) if expression.is_missing() => Value::Scalar(Blank),
                Some(expression) => {
                    self.observed_value_argument(sheet, at, expression, ordinal)?
                        .1
                }
                None => Value::Scalar(Boolean(false)),
            };
            let shape = Shape::inspect(&value).ok_or_else(|| {
                self.spill_unsupported(sheet, at, "IF requires one rectangular selected branch")
            })?;
            Ok(Some((value, shape)))
        };
        let when_true = branch(1, needs_true)?;
        let when_false = branch(2, needs_false)?;
        let mut output = condition_shape;
        for (_, shape) in when_true.iter().chain(when_false.iter()) {
            if condition_shape.rows == 1 {
                output.rows = output.rows.max(shape.rows);
            }
            if condition_shape.columns == 1 {
                output.columns = output.columns.max(shape.columns);
            }
        }
        self.rectangular_array(at, output, |session, row, column| {
            let condition =
                session.rectangular_element(&condition, condition_shape, row, column)?;
            let branch = match condition {
                Boolean(true) => when_true.as_ref(),
                Boolean(false) => when_false.as_ref(),
                Error(error) => return Ok(Error(error)),
                _ => return Err(EvaluationFailure::StalePlan),
            };
            let (value, shape) = branch.ok_or(EvaluationFailure::StalePlan)?;
            // IF pads exhausted non-singleton branch axes with #N/A, whereas
            // arithmetic operators use #VALUE! for the same shape mismatch.
            if (shape.rows != 1 && row >= shape.rows)
                || (shape.columns != 1 && column >= shape.columns)
            {
                return Ok(Error(NA));
            }
            session.rectangular_element(value, *shape, row, column)
        })
        .map(Value::Array)
    }
}
