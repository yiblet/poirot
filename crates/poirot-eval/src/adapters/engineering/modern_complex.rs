//! Modern complex scalar lifting and ordered complex collection reductions.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::{CallHost, NumericPolicy, ScalarOperand};
use crate::*;
use poirot_coercionv2::complex::{self as coercion, ComplexNumber, ComplexSuffix};
use poirot_functions_engineering::complex::{self as kernel, Unary};

/// Parsed coefficients plus explicit unit spelling for combination validation.
struct Parsed {
    value: ComplexNumber,
    unit: Option<ComplexSuffix>,
}
fn parse(host: &mut CallHost<'_, '_>, member: ScalarOperand) -> FunctionResult<Parsed> {
    if member.missing {
        return Err(host.unsupported(
            CapabilityId::FunctionAdapter,
            "complex functions have no verified omitted-argument coercion",
        ));
    }
    let scalar = match member.value {
        Text(text) => {
            host.check_text(text.len())?;
            host.charge_work((text.len() as u64).saturating_add(1))?;
            poirot_functions_core::Scalar::Text(text)
        }
        CellValue::TextUtf16(text) => {
            let text = host.project_text(poirot_model::TextRef::Utf16(text.units()))?;
            host.charge_work((text.len() as u64).saturating_add(1))?;
            poirot_functions_core::Scalar::Text(text)
        }
        Number(value) => poirot_functions_core::Scalar::Number(value),
        Boolean(value) => poirot_functions_core::Scalar::Boolean(value),
        Blank => poirot_functions_core::Scalar::Blank,
        Error(error) => poirot_functions_core::Scalar::Error(error),
    };
    let unit = match &scalar {
        poirot_functions_core::Scalar::Text(text) if text.ends_with('i') => Some(ComplexSuffix::I),
        poirot_functions_core::Scalar::Text(text) if text.ends_with('j') => Some(ComplexSuffix::J),
        _ => None,
    };
    let value = coercion::coercev2_complex_scalar(scalar)?.ok_or(VALUE_ERROR)?;
    Ok(Parsed { value, unit })
}
fn combine_unit(
    first: Option<ComplexSuffix>,
    next: Option<ComplexSuffix>,
) -> FunctionResult<Option<ComplexSuffix>> {
    match (first, next) {
        (Some(a), Some(b)) if a != b => Err(Num.into()),
        (Some(a), _) | (_, Some(a)) => Ok(Some(a)),
        _ => Ok(None),
    }
}
fn render(host: &mut CallHost<'_, '_>, value: ComplexNumber) -> FunctionResult<CellValue> {
    // Each finite binary64 component has bounded formatting scratch. Admit the
    // complete existing renderer's bound before allocating concatenated output.
    host.check_text(132)?;
    host.charge_work(132)?;
    let real = if value.real != 0.0 {
        host.coercev2_number_to_text(value.real, NumericPolicy::PinnedPoi)?
    } else {
        String::new()
    };
    let imaginary = if value.imaginary != 0.0 && value.imaginary != 1.0 && value.imaginary != -1.0 {
        host.coercev2_number_to_text(value.imaginary, NumericPolicy::PinnedPoi)?
    } else {
        String::new()
    };
    let text = coercion::coerce_complex_result(value, |number| {
        if number.to_bits() == value.real.to_bits() {
            real.clone()
        } else {
            imaginary.clone()
        }
    })?;
    host.check_text(text.len())?;
    Ok(Text(text))
}
fn unary(function: EngineeringFunction) -> Option<Unary> {
    use EngineeringFunction::*;
    Some(match function {
        Imconjugate => Unary::Conjugate,
        Imcos => Unary::Cos,
        Imcosh => Unary::Cosh,
        Imcot => Unary::Cot,
        Imcsc => Unary::Csc,
        Imcsch => Unary::Csch,
        Imexp => Unary::Exp,
        Imln => Unary::Ln,
        Imlog10 => Unary::Log10,
        Imlog2 => Unary::Log2,
        Imsec => Unary::Sec,
        Imsech => Unary::Sech,
        Imsin => Unary::Sin,
        Imsinh => Unary::Sinh,
        Imsqrt => Unary::Sqrt,
        Imtan => Unary::Tan,
        _ => return None,
    })
}
fn reduction(host: &mut CallHost<'_, '_>, product: bool) -> FunctionResult<Value> {
    let mut result = ComplexNumber {
        real: if product { 1.0 } else { 0.0 },
        imaginary: 0.0,
        suffix: ComplexSuffix::I,
    };
    let mut unit = None;
    for index in 0..host.len() {
        let length = host.collection_len(index)?;
        // This bounded dense traversal deliberately includes physically absent
        // cells and custom-reader values. Shared sparse aggregate admission is
        // a separate contract audit, not a complex-function shortcut.
        host.charge_work(length as u64)?;
        for offset in 0..length {
            let member = host.collection_cell(index, offset)?;
            let parsed = parse(host, member)?;
            unit = combine_unit(unit, parsed.unit)?;
            result = if product {
                kernel::multiply(result, parsed.value)?
            } else {
                kernel::add(result, parsed.value)?
            };
        }
    }
    result.suffix = unit.unwrap_or(ComplexSuffix::I);
    Ok(render(host, result)?.into())
}
/// Execute documented complex operations without widening the pinned profile.
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
    let result = if matches!(
        function,
        EngineeringFunction::Imsum | EngineeringFunction::Improduct
    ) {
        reduction(&mut host, function == EngineeringFunction::Improduct)
    } else {
        Lift::new(&mut host, arguments.len()).and_then(|lift| {
            lift.evaluate(&mut host, address, |host, row, column| {
                let member = lift.member(host, 0, row, column)?;
                let first = parse(host, member)?;
                if function == EngineeringFunction::Imabs {
                    return Ok(Number(kernel::magnitude(first.value)?));
                }
                if function == EngineeringFunction::Imargument {
                    return Ok(Number(kernel::argument(first.value)?));
                }
                if let Some(operation) = unary(function) {
                    return render(host, kernel::unary(first.value, operation)?);
                }
                let result = if function == EngineeringFunction::Impower {
                    // Real exponents are numeric arguments, not complex text or
                    // integer options; preserve ordinary attributed coercion.
                    let exponent = lift.number(host, 1, row, column, None)?;
                    kernel::power(first.value, exponent)?
                } else {
                    let member = lift.member(host, 1, row, column)?;
                    let second = parse(host, member)?;
                    let unit = combine_unit(first.unit, second.unit)?.unwrap_or(ComplexSuffix::I);
                    let mut result = if function == EngineeringFunction::Imdiv {
                        kernel::divide(first.value, second.value)?
                    } else if function == EngineeringFunction::Imsub {
                        kernel::subtract(first.value, second.value)?
                    } else {
                        return Err(VALUE_ERROR.into());
                    };
                    result.suffix = unit;
                    result
                };
                render(host, result)
            })
        })
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
