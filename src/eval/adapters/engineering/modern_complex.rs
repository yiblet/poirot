//! Modern complex scalar lifting and ordered complex collection reductions.
use crate::eval::adapters::elementwise::{
    Broadcast, Kernel, MemberContext, Optional, Variadic, WholeCollection,
};
use crate::eval::functions::registry::EngineeringFunction;
use crate::eval::host::{ArgumentCoercion, CallHost, NumericPolicy, ScalarOperand};
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::session::Session;
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellError::Num;
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};
use crate::model::{CellAddress, CellValue};
use poirot_functions::coercion::complex::{self as coercion, ComplexNumber, ComplexSuffix};
use poirot_functions::engineering::complex::{self as kernel, Unary};

/// A parsed complex operand that retains whether its imaginary-unit suffix was explicit.
/// Combination operations use that spelling to reject conflicting i and j operands.
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
            poirot_functions::scalar::Scalar::Text(text)
        }
        CellValue::TextUtf16(text) => {
            let text = ArgumentCoercion::new(host)
                .project_text(crate::model::TextRef::Utf16(text.units()))?;
            host.charge_work((text.len() as u64).saturating_add(1))?;
            poirot_functions::scalar::Scalar::Text(text)
        }
        Number(value) => poirot_functions::scalar::Scalar::Number(value),
        Boolean(value) => poirot_functions::scalar::Scalar::Boolean(value),
        Blank => poirot_functions::scalar::Scalar::Blank,
        Error(error) => poirot_functions::scalar::Scalar::Error(error),
    };
    let unit = match &scalar {
        poirot_functions::scalar::Scalar::Text(text) if text.ends_with('i') => {
            Some(ComplexSuffix::I)
        }
        poirot_functions::scalar::Scalar::Text(text) if text.ends_with('j') => {
            Some(ComplexSuffix::J)
        }
        _ => None,
    };
    let value = coercion::coerce_complex_scalar(scalar)?.ok_or(VALUE_ERROR)?;
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
        ArgumentCoercion::new(host).coerce_number_to_text(value.real, NumericPolicy::PinnedPoi)?
    } else {
        String::new()
    };
    let imaginary = if value.imaginary != 0.0 && value.imaginary != 1.0 && value.imaginary != -1.0 {
        ArgumentCoercion::new(host)
            .coerce_number_to_text(value.imaginary, NumericPolicy::PinnedPoi)?
    } else {
        String::new()
    };
    let text = coercion::convert_complex_result(value, |number| {
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
    use EngineeringFunction::{
        Imconjugate, Imcos, Imcosh, Imcot, Imcsc, Imcsch, Imexp, Imln, Imlog2, Imlog10, Imsec,
        Imsech, Imsin, Imsinh, Imsqrt, Imtan,
    };
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

/// IMSUM or IMPRODUCT kernel that reduces ordered collections once before returning its scalar result.
struct Reduction {
    product: bool,
    result: Option<CellValue>,
}

impl Kernel for Reduction {
    type Signature = Variadic<WholeCollection>;

    fn prepare(
        &mut self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<()> {
        let initial = ComplexNumber {
            real: if self.product { 1.0 } else { 0.0 },
            imaginary: 0.0,
            suffix: ComplexSuffix::I,
        };
        let (mut result, unit) = crate::eval::adapters::elementwise::collections::fold(
            host,
            arguments.iter(),
            (initial, None),
            |host, (result, unit), member| {
                let parsed = parse(host, member)?;
                *unit = combine_unit(*unit, parsed.unit)?;
                *result = if self.product {
                    kernel::multiply(*result, parsed.value)?
                } else {
                    kernel::add(*result, parsed.value)?
                };
                Ok(())
            },
        )?;
        result.suffix = unit.unwrap_or(ComplexSuffix::I);
        self.result = Some(render(host, result)?);
        Ok(())
    }

    fn apply(
        &self,
        _context: &mut MemberContext<'_, '_, '_>,
        _arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        self.result.clone().ok_or(VALUE_ERROR.into())
    }
}

/// Complex-function kernel that reads broadcast operands and applies operation-specific parsing and suffix rules.
/// Optional operands stay distinct so unary operations do not require binary inputs.
struct Complex(EngineeringFunction);

impl Kernel for Complex {
    type Signature = (Broadcast, Optional<Broadcast>, Optional<Broadcast>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use EngineeringFunction::{Imabs, Imaginary, Imargument, Imdiv, Impower, Imreal, Imsub};
        if self.0 == EngineeringFunction::Complex {
            let real = arguments.0.number(context, None)?;
            let imaginary = arguments
                .1
                .0
                .as_ref()
                .ok_or(VALUE_ERROR)?
                .number(context, None)?;
            let suffix = match &arguments.2.0 {
                Some(role) => {
                    let member = role.read(context)?;
                    if member.missing {
                        return Err(VALUE_ERROR.into());
                    }
                    let text =
                        ArgumentCoercion::new(context.host).coerce_scalar_text(&member.value)?;
                    coercion::coerce_complex_suffix(&text)?
                }
                None => ComplexSuffix::I,
            };
            return render(
                context.host,
                ComplexNumber {
                    real,
                    imaginary,
                    suffix,
                },
            );
        }
        let member = arguments.0.read(context)?;
        let first = parse(context.host, member)?;
        match self.0 {
            Imabs => return Ok(Number(kernel::magnitude(first.value)?)),
            Imargument => return Ok(Number(kernel::argument(first.value)?)),
            Imreal => return Ok(Number(first.value.real)),
            Imaginary => return Ok(Number(first.value.imaginary)),
            _ => {}
        }
        if let Some(operation) = unary(self.0) {
            return render(context.host, kernel::unary(first.value, operation)?);
        }
        let second = arguments.1.0.as_ref().ok_or(VALUE_ERROR)?;
        let result = if self.0 == Impower {
            kernel::power(first.value, second.number(context, None)?)?
        } else {
            let member = second.read(context)?;
            let second = parse(context.host, member)?;
            let unit = combine_unit(first.unit, second.unit)?.unwrap_or(ComplexSuffix::I);
            let mut result = match self.0 {
                Imdiv => kernel::divide(first.value, second.value)?,
                Imsub => kernel::subtract(first.value, second.value)?,
                _ => return Err(VALUE_ERROR.into()),
            };
            result.suffix = unit;
            result
        };
        render(context.host, result)
    }
}

/// Execute complex operations using the shared typed executor.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if matches!(
        function,
        EngineeringFunction::Imsum | EngineeringFunction::Improduct
    ) {
        super::dispatch::run(
            session,
            sheet,
            address,
            function,
            arguments,
            Reduction {
                product: function == EngineeringFunction::Improduct,
                result: None,
            },
        )
    } else {
        super::dispatch::run(
            session,
            sheet,
            address,
            function,
            arguments,
            Complex(function),
        )
    }
}
