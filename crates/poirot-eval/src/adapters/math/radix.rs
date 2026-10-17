//! Deferred general radix functions with explicit central argument policies.
use super::policy::{EvaluationPort, Failure, Scalar};
use poirot_coercionv2::radix::{
    coercev2_base_number, coercev2_base_padding, coercev2_decimal_scalar, coercev2_decimal_text,
    coercev2_decimal_text_capacity, coercev2_radix, coercev2_radix_number,
};
use poirot_functions_core::{CellError, CellValue};

fn read<P: EvaluationPort>(
    p: &mut P,
    argument: &P::Argument,
) -> Result<Scalar, Failure<P::Operational>> {
    p.charge_work(1).map_err(Failure::Operational)?;
    let scalar = p.resolve_scalar(argument)?.value;
    if let Scalar::Text(text) = &scalar {
        p.check_text(text.len()).map_err(Failure::Operational)?;
        p.charge_work(text.len() as u64)
            .map_err(Failure::Operational)?;
    }
    if let Scalar::Error(error) = scalar {
        return Err(error.into());
    }
    Ok(scalar)
}
/// Read operands in source order before domain validation. Input errors retain
/// identity, and operational failures never enter the catchable value channel.
pub fn evaluate<P: EvaluationPort>(
    p: &mut P,
    base: bool,
    args: &[P::Argument],
) -> Result<CellValue, Failure<P::Operational>> {
    if !(if base {
        (2..=3).contains(&args.len())
    } else {
        args.len() == 2
    }) {
        return Err(CellError::Value.into());
    }
    let first = read(p, args.first().ok_or(CellError::Value)?)?;
    let number = if base {
        Some(coercev2_radix_number(&first, |text| {
            p.coercev2_text_number(text)
        })?)
    } else {
        None
    };
    let radix = read(p, args.get(1).ok_or(CellError::Value)?)?;
    let radix = coercev2_radix_number(&radix, |text| p.coercev2_text_number(text))?;
    let padding = if let Some(argument) = args.get(2) {
        coercev2_radix_number(&read(p, argument)?, |text| p.coercev2_text_number(text))?
    } else {
        0.0
    };
    let radix = coercev2_radix(radix)?;
    if let Some(number) = number {
        let number = coercev2_base_number(number)?;
        let padding = coercev2_base_padding(padding)?;
        let length = poirot_functions_math::radix::base_length(number, radix, padding)?;
        p.check_collection(length).map_err(Failure::Operational)?;
        p.check_text(length).map_err(Failure::Operational)?;
        p.charge_work(length as u64).map_err(Failure::Operational)?;
        Ok(CellValue::Text(poirot_functions_math::radix::base(
            number, radix, padding,
        )?))
    } else {
        let bytes = coercev2_decimal_text_capacity(&first);
        p.check_text(bytes).map_err(Failure::Operational)?;
        p.check_collection(bytes).map_err(Failure::Operational)?;
        p.charge_work(bytes as u64).map_err(Failure::Operational)?;
        let text = coercev2_decimal_scalar(&first)?;
        Ok(CellValue::Number(coercev2_decimal_text(&text, radix)?))
    }
}
