//! Typed signed conversion and elementwise engineering arithmetic.
use crate::eval::adapters::elementwise::{Broadcast, Kernel, MemberContext, Optional};
use crate::eval::functions::registry::EngineeringFunction;
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR, Value};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::{NA, Num};
use crate::model::CellValue::{Error, Number, Text};
use crate::model::{CellAddress, CellValue};
use poirot_functions::engineering::{
    radix::{self, BitOperation, Radix, Shift},
    units,
};

/// Identify the modern engineering operations handled in this module.
pub(super) fn handles(function: EngineeringFunction) -> bool {
    use EngineeringFunction::{
        Bin2dec, Bin2hex, Bin2oct, Bitand, Bitlshift, Bitor, Bitrshift, Bitxor, Convert, Dec2bin,
        Dec2hex, Dec2oct, Hex2bin, Hex2dec, Hex2oct, Oct2bin, Oct2dec, Oct2hex,
    };
    matches!(
        function,
        Bin2dec
            | Hex2dec
            | Oct2dec
            | Dec2bin
            | Dec2hex
            | Bin2hex
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

fn domain(error: poirot_functions::engineering::EngineeringError) -> FunctionError {
    match error {
        poirot_functions::engineering::EngineeringError::Num => Num.into(),
        poirot_functions::engineering::EngineeringError::Value => VALUE_ERROR.into(),
    }
}

fn destination(function: EngineeringFunction) -> Option<Radix> {
    use EngineeringFunction::{
        Bin2hex, Bin2oct, Dec2bin, Dec2hex, Dec2oct, Hex2bin, Hex2oct, Oct2bin, Oct2hex,
    };
    match function {
        Bin2hex | Oct2hex | Dec2hex => Some(Radix::Hexadecimal),
        Dec2bin => Some(Radix::Binary),
        Bin2oct | Dec2oct | Hex2oct => Some(Radix::Octal),
        Hex2bin | Oct2bin => Some(Radix::Binary),
        _ => None,
    }
}

fn coerce_engineering_text(
    host: &mut CallHost<'_, '_>,
    value: &CellValue,
) -> FunctionResult<String> {
    let text = ArgumentCoercion::new(host).coerce_scalar_text(value)?;
    host.check_text(text.len())?;
    host.charge_work((text.len() as u64).saturating_add(1))?;
    Ok(text)
}

/// Formats a signed binary, octal, or hexadecimal result for each broadcast member.
/// It converts the optional width before calling the independent radix encoder.
struct Conversion(EngineeringFunction);

impl Kernel for Conversion {
    type Signature = (Broadcast, Optional<Broadcast>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use EngineeringFunction::{
            Bin2hex, Bin2oct, Dec2bin, Dec2hex, Dec2oct, Hex2bin, Hex2oct, Oct2bin, Oct2hex,
        };
        let target = destination(self.0).ok_or(VALUE_ERROR)?;
        let number = if matches!(self.0, Dec2oct | Dec2bin | Dec2hex) {
            target
                .integer(arguments.0.number(context, None)?)
                .map_err(domain)?
        } else {
            let member = arguments.0.read(context)?;
            if member.missing {
                return Err(VALUE_ERROR.into());
            }
            let text = coerce_engineering_text(context.host, &member.value)?;
            let source = match self.0 {
                Bin2hex | Bin2oct => Radix::Binary,
                Hex2bin | Hex2oct => Radix::Hexadecimal,
                Oct2bin | Oct2hex => Radix::Octal,
                _ => return Err(VALUE_ERROR.into()),
            };
            let number = source.decode(&text).map_err(domain)?;
            target.integer(number as f64).map_err(domain)?;
            number
        };
        // Microsoft BIN2HEX/DEC2BIN document nonnumeric places as VALUE errors;
        // negative numbers ignore the successfully coerced width in the kernel.
        // https://support.microsoft.com/en-us/excel/functions/bin2hex-function
        // https://support.microsoft.com/en-au/excel/functions/dec2bin-function
        let places = match &arguments.1.0 {
            Some(role) => {
                let member = role.read(context)?;
                if member.missing {
                    None
                } else {
                    Some(ArgumentCoercion::new(context.host).coerce_scalar_number(&member.value)?)
                }
            }
            None => None,
        };
        context.host.check_text(10)?;
        target.encode(number, places).map(Text).map_err(domain)
    }
}

/// Engineering kernel for bit arithmetic, unit conversion, and signed radix decoding.
/// Typed operands preserve each operation's numeric or text coercion order within shared broadcast traversal.
struct Arithmetic(EngineeringFunction);

impl Kernel for Arithmetic {
    type Signature = (Broadcast, Optional<Broadcast>, Optional<Broadcast>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use EngineeringFunction::{
            Bin2dec, Bitand, Bitlshift, Bitor, Bitrshift, Bitxor, Convert, Hex2dec, Oct2dec,
        };
        if matches!(self.0, Bin2dec | Hex2dec | Oct2dec) {
            let member = arguments.0.read(context)?;
            if member.missing {
                return Err(VALUE_ERROR.into());
            }
            let text = coerce_engineering_text(context.host, &member.value)?;
            let source = match self.0 {
                Bin2dec => Radix::Binary,
                Hex2dec => Radix::Hexadecimal,
                _ => Radix::Octal,
            };
            return source
                .decode(&text)
                .map(|number| Number(number as f64))
                .map_err(domain);
        }
        let a = arguments.0.number(context, None)?;
        let second = arguments.1.0.as_ref().ok_or(VALUE_ERROR)?;
        if self.0 == Convert {
            let from = second.read(context)?;
            let from = match from.value {
                value @ (Text(_) | CellValue::TextUtf16(_)) => {
                    coerce_engineering_text(context.host, &value)?
                }
                Error(error) => return Err(error.into()),
                _ => return Err(VALUE_ERROR.into()),
            };
            let to = arguments.2.0.as_ref().ok_or(VALUE_ERROR)?.read(context)?;
            let to = match to.value {
                value @ (Text(_) | CellValue::TextUtf16(_)) => {
                    coerce_engineering_text(context.host, &value)?
                }
                Error(error) => return Err(error.into()),
                _ => return Err(VALUE_ERROR.into()),
            };
            return units::convert(a, &from, &to)
                .map(Number)
                .map_err(|error| match error {
                    poirot_functions::engineering::ConversionError::NotAvailable => NA.into(),
                    poirot_functions::engineering::ConversionError::Number => Num.into(),
                });
        }
        let b = second.number(context, None)?;
        Ok(Number(match self.0 {
            Bitand => radix::bitwise(a, b, BitOperation::And).map_err(domain)?,
            Bitor => radix::bitwise(a, b, BitOperation::Or).map_err(domain)?,
            Bitxor => radix::bitwise(a, b, BitOperation::Xor).map_err(domain)?,
            Bitlshift => radix::shift(a, b, Shift::Left).map_err(domain)?,
            Bitrshift => radix::shift(a, b, Shift::Right).map_err(domain)?,
            _ => return Err(VALUE_ERROR.into()),
        }))
    }
}

/// Execute signed conversion and arithmetic through shared typed argument admission.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if destination(function).is_some() {
        super::dispatch::run(
            session,
            sheet,
            address,
            function,
            arguments,
            Conversion(function),
        )
    } else {
        super::dispatch::run(
            session,
            sheet,
            address,
            function,
            arguments,
            Arithmetic(function),
        )
    }
}
