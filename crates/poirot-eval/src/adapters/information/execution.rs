//! Information family handles error values rather than propagating them uniformly.
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
            // LogicalFunction classifies the resolved ValueEval without coercion.
            // MissingArgEval is not BlankEval, including when selected by IFS.
            // OperandResolver selection errors are classified as ErrorEval;
            // operational failures still abort the evaluation.
            let (value, missing) =
                match CallHost::new(self, s, at, args).scalar_data(0, MissingPolicy::Blank) {
                    Ok(operand) => (operand.value, operand.missing),
                    Err(FunctionFailure::Spreadsheet(error)) => (Error(error), false),
                    Err(FunctionFailure::Operational(failure)) => return Err(failure),
                };
            let result = match function {
                IsError => !missing && matches!(value, Error(_)),
                IsErr => !missing && matches!(value, Error(error) if error != NA),
                IsNa => !missing && value == Error(NA),
                IsNumber => !missing && matches!(value, Number(_)),
                IsText => !missing && value.text_ref().is_some(),
                IsBlank => !missing && value == Blank,
                IsLogical => !missing && matches!(value, Boolean(_)),
                IsNonText => missing || value.text_ref().is_none(),
                // SAFETY: The classifier-only match is guarded by those exact variants, and that branch returns before the later non-classifier match.
                _ => unreachable!("classifier dispatch"),
            };
            return Ok(Boolean(result).into());
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
            N => coerce_information_n(value).into(),
            T => coerce_information_t(value).into(),
            IsError | IsErr | IsNa | IsNumber | IsText | IsBlank | IsLogical | IsNonText => {
                // SAFETY: The classifier-only match is guarded by those exact variants, and that branch returns before the later non-classifier match.
                unreachable!("classifiers return before legacy coercion")
            }
        })
    }
}

/// N converts booleans to numbers and other nonnumeric, nonerror values to zero.
fn coerce_information_n(value: CellValue) -> CellValue {
    match value {
        Number(_) | Error(_) => value,
        Boolean(b) => Number(u8::from(b).into()),
        _ => Number(0.),
    }
}

/// T retains text/errors and converts every other scalar to empty text.
fn coerce_information_t(value: CellValue) -> CellValue {
    match value {
        Text(_) | CellValue::TextUtf16(_) | Error(_) => value,
        _ => Text(String::new()),
    }
}
