//! Bounded numeric lifting for documented real engineering special functions.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::CallHost;
use crate::*;
use poirot_functions_engineering::special::{self as kernel, Bessel};

/// Recognize the documented special-function execution path.
pub(super) fn handles(function: EngineeringFunction) -> bool {
    use EngineeringFunction::*;
    matches!(
        function,
        Besseli | Besselk | Bessely | Erf | ErfPrecise | Erfc | ErfcPrecise | Gestep
    )
}
/// Evaluate scalar or broadcast members while retaining host failure identity.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Engineering(function)
        .descriptor()
        .arity
        .accepts(arguments.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, arguments, arguments.len());
    let result = Lift::new(&mut host, arguments.len()).and_then(|lift| {
        lift.evaluate(&mut host, address, |host, row, column| {
            let x = lift.number(host, 0, row, column, None)?;
            use EngineeringFunction::*;
            let value = match function {
                Gestep => kernel::gestep(x, lift.number(host, 1, row, column, Some(0.0))?),
                Erf => {
                    if arguments.len() == 1 {
                        kernel::erf(x)
                    } else {
                        kernel::erf_interval(x, lift.number(host, 1, row, column, Some(0.0))?)
                    }
                }
                ErfPrecise => kernel::erf(x),
                Erfc | ErfcPrecise => kernel::erfc(x),
                Besseli | Besselk | Bessely => {
                    let order = lift.number(host, 1, row, column, None)?;
                    let kind = match function {
                        Besseli => Bessel::ModifiedFirst,
                        Besselk => Bessel::ModifiedSecond,
                        _ => Bessel::Second,
                    };
                    kernel::bessel(kind, x, order, |units| host.charge_work(units)).map_err(
                        |error| match error {
                            kernel::Failure::Number => FunctionFailure::Spreadsheet(Num),
                            kernel::Failure::Operational(error) => error.into(),
                            kernel::Failure::ComputationalLimit => host.unsupported(
                                CapabilityId::FunctionAdapter,
                                "Bessel computation exceeds the bounded order/series envelope",
                            ),
                        },
                    )?
                }
                _ => return Err(VALUE_ERROR.into()),
            };
            if value.is_finite() {
                Ok(Number(value))
            } else {
                Err(Num.into())
            }
        })
    });
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
