//! Information family handles error values rather than propagating them uniformly.
use crate::array_operator::Shape;
use crate::host::{CallHost, MissingPolicy};
use crate::*;
impl Session<'_> {
    /// Inspect scalar types and errors using each information builtin's coercion rules.
    pub(crate) fn information_function(
        &mut self,
        s: usize,
        at: CellAddress,
        function: InformationFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        use InformationFunction::*;
        if !FunctionId::Information(function)
            .descriptor()
            .arity
            .accepts(args.len())
        {
            return Ok(err(VALUE_ERROR));
        }
        if function == Na {
            return Ok(err(NA));
        }
        if matches!(
            function,
            IsError | IsErr | IsNa | IsNumber | IsText | IsBlank | IsLogical | IsNonText
        ) {
            self.read(DependencyRead::BuiltinPolicy);
            let modern = self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation;
            let mut host = if modern {
                CallHost::new_inheriting(self, s, at, args)
            } else {
                CallHost::new(self, s, at, args)
            };
            if modern {
                let retains_array = match semantic(host.retains_array(0))? {
                    Ok(retains_array) => retains_array,
                    Err(error) => {
                        return information_predicate(function, &Error(error), false)
                            .map(Value::from);
                    }
                };
                if retains_array {
                    let operand = match semantic(host.argument(0))? {
                        Ok(operand) => operand,
                        Err(error) => {
                            return information_predicate(function, &Error(error), false)
                                .map(Value::from);
                        }
                    };
                    drop(host);
                    let shape = Shape::inspect(operand.value()).ok_or_else(|| {
                        self.spill_unsupported(
                            s,
                            at,
                            "information predicate requires one bounded rectangular operand",
                        )
                    })?;
                    let rows = self.rectangular_array(at, shape, |session, row, column| {
                        let value =
                            session.rectangular_element(operand.value(), shape, row, column)?;
                        information_predicate(function, &value, operand.missing_at(row, column))
                    })?;
                    return Ok(Value::Array(rows));
                }
            }
            // MissingArgEval is not BlankEval, including when selected by IFS.
            // Spreadsheet errors remain inspectable data; operational failures escape.
            let (value, missing) = match host.scalar_data(0, MissingPolicy::Blank) {
                Ok(operand) => (operand.value, operand.missing),
                Err(FunctionFailure::Spreadsheet(error)) => (Error(error), false),
                Err(FunctionFailure::Operational(failure)) => return Err(failure),
            };
            return information_predicate(function, &value, missing).map(Value::from);
        }
        let value = self.argument(s, at, args, 0)?;
        Ok(match function {
            Na => err(NA),
            Type => Number(match value {
                Number(_) | Blank => 1.,
                Text(_) | CellValue::TextUtf16(_) => 2.,
                Boolean(_) => 4.,
                Error(_) => 16.,
            })
            .into(),
            N => coercev2_information_n(value).into(),
            T => coercev2_information_t(value).into(),
            IsError | IsErr | IsNa | IsNumber | IsText | IsBlank | IsLogical | IsNonText => {
                // SAFETY: The classifier-only match is guarded by those exact variants, and that branch returns before the later non-classifier match.
                unreachable!("classifiers return before legacy coercion")
            }
        })
    }
}

/// Exact type inspection neither parses text nor converts blank, omission, or errors.
fn information_predicate(
    function: InformationFunction,
    value: &CellValue,
    missing: bool,
) -> EvalResult<CellValue> {
    use InformationFunction::*;
    let result = match function {
        IsError => !missing && matches!(value, Error(_)),
        IsErr => !missing && matches!(value, Error(error) if *error != NA),
        IsNa => !missing && *value == Error(NA),
        IsNumber => !missing && matches!(value, Number(_)),
        IsText => !missing && value.text_ref().is_some(),
        IsBlank => !missing && *value == Blank,
        IsLogical => !missing && matches!(value, Boolean(_)),
        IsNonText => missing || value.text_ref().is_none(),
        _ => return Err(EvaluationFailure::StalePlan),
    };
    Ok(Boolean(result))
}

/// N converts booleans to numbers and other nonnumeric, nonerror values to zero.
fn coercev2_information_n(value: CellValue) -> CellValue {
    poirot_coercionv2::compatibility::coercev2_information_n(value)
}

/// T retains text/errors and converts every other scalar to empty text.
fn coercev2_information_t(value: CellValue) -> CellValue {
    poirot_coercionv2::compatibility::coercev2_information_t(value)
}
