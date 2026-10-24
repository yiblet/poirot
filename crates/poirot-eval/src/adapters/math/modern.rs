//! Documented scalar math lifting and factorial-ratio aggregation.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::{CallHost, NumericPolicy, OperandOrigin};
use crate::*;
use poirot_functions_math::modern::{self, Reciprocal};

fn scalar(function: MathFunction, first: f64, second: f64) -> Result<f64, CellError> {
    use MathFunction::*;
    match function {
        Acot => modern::acot(first),
        Acoth => modern::acoth(first),
        Cot => modern::reciprocal(first, Reciprocal::Cot),
        Coth => modern::reciprocal(first, Reciprocal::Coth),
        Csc => modern::reciprocal(first, Reciprocal::Csc),
        Csch => modern::reciprocal(first, Reciprocal::Csch),
        Sec => modern::reciprocal(first, Reciprocal::Sec),
        Sech => modern::reciprocal(first, Reciprocal::Sech),
        IsoCeiling if first == 0.0 || second == 0.0 => Ok(0.0),
        IsoCeiling => poirot_functions_math::ceiling_precise(first, second),
        _ => Err(VALUE_ERROR),
    }
}
fn multinomial(host: &mut CallHost<'_, '_>) -> FunctionResult<Value> {
    let mut result = modern::Multinomial::new();
    for index in 0..host.len() {
        let length = host.collection_len(index)?;
        host.charge_work(length as u64)?;
        for offset in 0..length {
            let member = host.collection_cell(index, offset)?;
            if member.origin != OperandOrigin::Direct
                && matches!(
                    member.value,
                    Blank | Boolean(_) | Text(_) | CellValue::TextUtf16(_)
                )
            {
                continue;
            }
            let number = host.coercev2_scalar_number(&member.value, NumericPolicy::PinnedPoi)?;
            result.push(number, || {
                host.charge_work(1).map_err(FunctionFailure::from)
            })?;
        }
    }
    Ok(Number(result.finish()).into())
}
/// Execute the new math utilities without changing the pinned math paths.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: MathFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Math(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = if function == MathFunction::Multinomial {
        multinomial(&mut host)
    } else {
        Lift::new(&mut host, args.len()).and_then(|lift| {
            lift.evaluate(&mut host, address, |host, row, column| {
                let first = lift.number(host, 0, row, column, Some(0.0))?;
                let second = if function == MathFunction::IsoCeiling {
                    lift.number(host, 1, row, column, Some(1.0))?
                } else {
                    0.0
                };
                Ok(Number(scalar(function, first, second)?))
            })
        })
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
