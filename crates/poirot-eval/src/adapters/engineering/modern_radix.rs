//! Demand-sensitive signed conversion and elementwise engineering arithmetic.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::CallHost;
use crate::*;
use poirot_functions_engineering::{
    radix::{self, BitOperation, Radix, Shift},
    units,
};

/// Identify the modern engineering operations handled in this module.
pub(super) fn handles(function: EngineeringFunction) -> bool {
    use EngineeringFunction::*;
    matches!(
        function,
        Bin2hex
            | Bin2oct
            | Bitand
            | Bitlshift
            | Bitor
            | Bitrshift
            | Bitxor
            | Convert
            | Dec2oct
            | Hex2bin
            | Hex2oct
            | Oct2bin
            | Oct2hex
    )
}
fn domain(error: poirot_functions_engineering::EngineeringError) -> FunctionFailure {
    match error {
        poirot_functions_engineering::EngineeringError::Num => Num.into(),
        poirot_functions_engineering::EngineeringError::Value => VALUE_ERROR.into(),
    }
}
fn destination(function: EngineeringFunction) -> Option<Radix> {
    use EngineeringFunction::*;
    match function {
        Bin2hex | Oct2hex => Some(Radix::Hexadecimal),
        Bin2oct | Dec2oct | Hex2oct => Some(Radix::Octal),
        Hex2bin | Oct2bin => Some(Radix::Binary),
        _ => None,
    }
}
fn text(
    host: &mut CallHost<'_, '_>,
    lift: &Lift,
    index: usize,
    row: usize,
    column: usize,
) -> FunctionResult<String> {
    let member = lift.member(host, index, row, column)?;
    if member.missing {
        return Err(VALUE_ERROR.into());
    }
    coercev2_engineering_text(host, &member.value)
}
fn coercev2_engineering_text(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<String> {
    let text = host.coercev2_scalar_text(value)?;
    host.check_text(text.len())?;
    host.charge_work((text.len() as u64).saturating_add(1))?;
    Ok(text)
}
fn unit_text(
    host: &mut CallHost<'_, '_>,
    lift: &Lift,
    index: usize,
    row: usize,
    column: usize,
) -> FunctionResult<String> {
    let member = lift.member(host, index, row, column)?;
    match member.value {
        value @ (Text(_) | CellValue::TextUtf16(_)) => coercev2_engineering_text(host, &value),
        Error(error) => Err(error.into()),
        _ => Err(VALUE_ERROR.into()),
    }
}
fn primary(
    host: &mut CallHost<'_, '_>,
    lift: &Lift,
    function: EngineeringFunction,
    row: usize,
    column: usize,
) -> FunctionResult<i64> {
    use EngineeringFunction::*;
    let target = destination(function).ok_or(VALUE_ERROR)?;
    if function == Dec2oct {
        return target
            .integer(lift.number(host, 0, row, column, None)?)
            .map_err(domain);
    }
    let text = text(host, lift, 0, row, column)?;
    let source = match function {
        Bin2hex | Bin2oct => Radix::Binary,
        Hex2bin | Hex2oct => Radix::Hexadecimal,
        Oct2bin | Oct2hex => Radix::Octal,
        _ => return Err(VALUE_ERROR.into()),
    };
    let number = source.decode(&text).map_err(domain)?;
    target.integer(number as f64).map_err(domain)?;
    Ok(number)
}
fn conversion(
    host: &mut CallHost<'_, '_>,
    address: CellAddress,
    function: EngineeringFunction,
) -> FunctionResult<Value> {
    let primary_lift = Lift::new(host, 1)?;
    let (rows, columns) = primary_lift.dimensions();
    let mut needs_places = false;
    if host.len() > 1 {
        for row in 0..rows {
            for column in 0..columns {
                host.charge_work(1)?;
                match primary(host, &primary_lift, function, row, column) {
                    Ok(number) if number >= 0 => {
                        needs_places = true;
                        break;
                    }
                    Ok(_) | Err(FunctionFailure::Spreadsheet(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            if needs_places {
                break;
            }
        }
    }
    // Do not discover geometry by executing a semantically ignored operand.
    let lift = if needs_places {
        Lift::new(host, host.len())?
    } else {
        primary_lift
    };
    lift.evaluate(host, address, |host, row, column| {
        let number = primary(host, &lift, function, row, column)?;
        let places = if number >= 0 && host.len() > 1 {
            {
                let member = lift.member(host, 1, row, column)?;
                if member.missing {
                    None
                } else {
                    Some(host.coercev2_scalar_number(
                        &member.value,
                        crate::host::NumericPolicy::PinnedPoi,
                    )?)
                }
            }
        } else {
            None
        };
        // Every result is at most ten ASCII characters. Admit before formatting.
        host.check_text(10)?;
        Ok(Text(
            destination(function)
                .ok_or(VALUE_ERROR)?
                .encode(number, places)
                .map_err(domain)?,
        ))
    })
}
/// Execute documented functions through bounded scalar lifting and deferred reads.
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
    let result = if destination(function).is_some() {
        conversion(&mut host, address, function)
    } else {
        Lift::new(&mut host, arguments.len()).and_then(|lift| {
            lift.evaluate(&mut host, address, |host, row, column| {
                let a = lift.number(host, 0, row, column, None)?;
                use EngineeringFunction::*;
                if function == Convert {
                    let from = unit_text(host, &lift, 1, row, column)?;
                    let to = unit_text(host, &lift, 2, row, column)?;
                    return units::convert(a, &from, &to).map(Number).map_err(
                        |error| match error {
                            units::ConversionError::NotAvailable => NA.into(),
                            units::ConversionError::Number => Num.into(),
                        },
                    );
                }
                let b = lift.number(host, 1, row, column, None)?;
                Ok(Number(match function {
                    Bitand => radix::bitwise(a, b, BitOperation::And).map_err(domain)?,
                    Bitor => radix::bitwise(a, b, BitOperation::Or).map_err(domain)?,
                    Bitxor => radix::bitwise(a, b, BitOperation::Xor).map_err(domain)?,
                    Bitlshift => radix::shift(a, b, Shift::Left).map_err(domain)?,
                    Bitrshift => radix::shift(a, b, Shift::Right).map_err(domain)?,
                    _ => return Err(VALUE_ERROR.into()),
                }))
            })
        })
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
