//! General radix operations with typed broadcast argument roles.
use crate::eval::adapters::elementwise::{
    Broadcast, Kernel, MemberContext, NumericParameter as P, NumericSignature, Optional,
};
use crate::eval::*;
use poirot_functions::coercion::radix::{
    coercev2_base_number, coercev2_base_padding, coercev2_decimal_scalar, coercev2_decimal_text,
    coercev2_decimal_text_capacity, coercev2_radix,
};
use poirot_functions::scalar::Scalar;

/// BASE or DECIMAL kernel for each broadcast output member.
/// It converts radix and padding controls before invoking independent formatting or parsing algorithms.
pub(super) struct Radix(pub(super) MathFunction);
impl Kernel for Radix {
    type Signature = (Broadcast, Broadcast, Optional<Broadcast>);
    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (first, radix, padding): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        if self.0 == MathFunction::Base {
            let numbers =
                NumericSignature::new(&[P::constant(0.0), P::constant(0.0), P::constant(0.0)])
                    .numbers_roles(context, [Some(*first), Some(*radix), padding.0])?;
            let radix = coercev2_radix(numbers.get(1)?)?;
            let number = coercev2_base_number(numbers.get(0)?)?;
            let padding = coercev2_base_padding(numbers.get(2)?)?;
            let length = poirot_functions::math::radix::base_length(number, radix, padding)?;
            context.host.check_values(length)?;
            context.host.check_text(length)?;
            context.host.charge_work(length as u64)?;
            return Ok(Text(poirot_functions::math::radix::base(
                number, radix, padding,
            )?));
        }
        let first = first.read(context)?;
        let first = if first.missing {
            Scalar::Missing
        } else {
            match first.value {
                Number(number) => Scalar::Number(number),
                Boolean(value) => Scalar::Boolean(value),
                Text(text) => Scalar::Text(text),
                CellValue::TextUtf16(text) => Scalar::Text(
                    context
                        .host
                        .project_text(crate::model::TextRef::Utf16(text.units()))?,
                ),
                Blank => Scalar::Blank,
                Error(error) => return Err(error.into()),
            }
        };
        let radix = radix.number(context, Some(0.0))?;
        let radix = coercev2_radix(radix)?;
        let bytes = coercev2_decimal_text_capacity(&first);
        context.host.check_text(bytes)?;
        context.host.check_values(bytes)?;
        context.host.charge_work(bytes as u64)?;
        Ok(Number(coercev2_decimal_text(
            &coercev2_decimal_scalar(&first)?,
            radix,
        )?))
    }
}
