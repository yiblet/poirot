use crate::{
    CellError, CellValue, CoerceV2, CoercionV2Context, CoercionV2Failure, Input, Origin, TextRef,
    coercev2,
    parsing::{coercev2_decimal_text, coercev2_text_view},
};
use std::{borrow::Cow, marker::PhantomData};

fn required_value(input: Input<'_>) -> Result<&CellValue, CoercionV2Failure> {
    match input.value() {
        Some(CellValue::Error(error)) => Err(CoercionV2Failure::Worksheet(*error)),
        Some(value) => Ok(value),
        None => Err(CoercionV2Failure::mismatch("present scalar")),
    }
}

/// Require an existing number, without changing its IEEE-754 payload.
/// Finiteness/domain validation belongs to a consuming rule, not type inspection.
pub struct ExactNumber;

impl CoerceV2<'_> for ExactNumber {
    type Output = f64;

    fn coercev2_input(
        input: Input<'_>,
        _: &mut CoercionV2Context,
    ) -> Result<f64, CoercionV2Failure> {
        match required_value(input)? {
            CellValue::Number(number) => Ok(*number),
            _ => Err(CoercionV2Failure::mismatch("existing number")),
        }
    }
}

/// Require an existing boolean; do not interpret numbers or text.
pub struct ExactBoolean;

impl CoerceV2<'_> for ExactBoolean {
    type Output = bool;

    fn coercev2_input(
        input: Input<'_>,
        _: &mut CoercionV2Context,
    ) -> Result<bool, CoercionV2Failure> {
        match required_value(input)? {
            CellValue::Boolean(boolean) => Ok(*boolean),
            _ => Err(CoercionV2Failure::mismatch("existing boolean")),
        }
    }
}

/// Borrow exact text, retaining isolated UTF-16 surrogates without allocation.
pub struct ExactText;

impl<'a> CoerceV2<'a> for ExactText {
    type Output = TextRef<'a>;

    fn coercev2_input(
        input: Input<'a>,
        _: &mut CoercionV2Context,
    ) -> Result<Self::Output, CoercionV2Failure> {
        required_value(input)?
            .text_ref()
            .ok_or_else(|| CoercionV2Failure::mismatch("existing text"))
    }
}

/// Explicitly request a worksheet error as data, rather than propagating it.
pub struct ExactError;

impl CoerceV2<'_> for ExactError {
    type Output = CellError;

    fn coercev2_input(
        input: Input<'_>,
        _: &mut CoercionV2Context,
    ) -> Result<CellError, CoercionV2Failure> {
        match input.value() {
            Some(CellValue::Error(error)) => Ok(*error),
            _ => Err(CoercionV2Failure::mismatch("existing worksheet error")),
        }
    }
}

/// Preserve scalar type, omission, error value, and origin for a selector/predicate.
pub struct Identity;

impl<'a> CoerceV2<'a> for Identity {
    type Output = Input<'a>;

    fn coercev2_input(
        input: Input<'a>,
        _: &mut CoercionV2Context,
    ) -> Result<Input<'a>, CoercionV2Failure> {
        Ok(input)
    }
}

/// Finite number, blank-as-zero, boolean-as-zero/one, or finite decimal text.
/// This is an explicit decimal-only policy, not full Excel arithmetic or VALUE.
pub struct DecimalNumber;

impl CoerceV2<'_> for DecimalNumber {
    type Output = f64;

    fn coercev2_input(
        input: Input<'_>,
        context: &mut CoercionV2Context,
    ) -> Result<f64, CoercionV2Failure> {
        let value = required_value(input)?;
        match value {
            CellValue::Number(number) if number.is_finite() => Ok(*number),
            CellValue::Number(_) => Err(CoercionV2Failure::Worksheet(CellError::Num)),
            CellValue::Blank => Ok(0.0),
            CellValue::Boolean(boolean) => Ok(f64::from(u8::from(*boolean))),
            _ => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionV2Failure::mismatch("finite decimal number"))?;
                let text = coercev2_text_view(text, context)?;
                coercev2_decimal_text(&text)
                    .ok_or_else(|| CoercionV2Failure::mismatch("finite decimal number"))
            }
        }
    }
}

/// Boolean, finite numeric truthiness, blank-as-false, or exact ASCII TRUE/FALSE
/// spelling (case-insensitive, with no trimming). This is not AND/OR collection policy.
pub struct LogicalBoolean;

impl CoerceV2<'_> for LogicalBoolean {
    type Output = bool;

    fn coercev2_input(
        input: Input<'_>,
        context: &mut CoercionV2Context,
    ) -> Result<bool, CoercionV2Failure> {
        match required_value(input)? {
            CellValue::Boolean(boolean) => Ok(*boolean),
            CellValue::Blank => Ok(false),
            CellValue::Number(number) if number.is_finite() => Ok(*number != 0.0),
            CellValue::Number(_) => Err(CoercionV2Failure::Worksheet(CellError::Num)),
            value => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionV2Failure::mismatch("logical scalar"))?;
                let length = u64::try_from(text.storage_bytes())
                    .map_err(|_| CoercionV2Failure::Limit(crate::Resource::Work))?;
                context.charge(length)?;
                let equals = |expected: &str| {
                    text.units()
                        .map(|unit| {
                            u8::try_from(unit)
                                .ok()
                                .map(|byte| byte.to_ascii_lowercase())
                        })
                        .eq(expected.bytes().map(Some))
                };
                if equals("true") {
                    Ok(true)
                } else if equals("false") {
                    Ok(false)
                } else {
                    Err(CoercionV2Failure::mismatch("logical scalar"))
                }
            }
        }
    }
}

/// Unicode scalar formula text; no cell display format is consulted.
/// Use ExactText when exact UTF-16 units, including isolated surrogates, are required.
pub struct FormulaText;

impl<'a> CoerceV2<'a> for FormulaText {
    type Output = Cow<'a, str>;

    fn coercev2_input(
        input: Input<'a>,
        context: &mut CoercionV2Context,
    ) -> Result<Self::Output, CoercionV2Failure> {
        match required_value(input)? {
            CellValue::Blank => Ok(Cow::Borrowed("")),
            CellValue::Boolean(true) => Ok(Cow::Borrowed("TRUE")),
            CellValue::Boolean(false) => Ok(Cow::Borrowed("FALSE")),
            CellValue::Number(number) if number.is_finite() => {
                context.admit_text(poirot_numeric::number_text::ALLOCATION_BOUND)?;
                Ok(Cow::Owned(poirot_numeric::number_text::render(
                    *number,
                    context.number_text_source,
                )))
            }
            CellValue::Number(_) => Err(CoercionV2Failure::Worksheet(CellError::Num)),
            value => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionV2Failure::mismatch("formula text"))?;
                coercev2_text_view(text, context)
            }
        }
    }
}

/// Nonnegative finite decimal input whose truncated integer fits i32.
/// Functions with other bounds must request a distinct rule.
pub struct TextCount;

impl CoerceV2<'_> for TextCount {
    type Output = i32;

    fn coercev2_input(
        input: Input<'_>,
        context: &mut CoercionV2Context,
    ) -> Result<i32, CoercionV2Failure> {
        let number = coercev2::<DecimalNumber>(input, context)?;
        if number < 0.0 {
            return Err(CoercionV2Failure::Worksheet(CellError::Value));
        }
        let number = crate::compatibility::coercev2_text_integer_number(
            number,
            crate::compatibility::TextIntegerPolicy::Checked,
        )
        .map_err(CoercionV2Failure::Worksheet)?;
        Ok(number.trunc() as i32)
    }
}

/// Only omitted syntax produces None; a referenced blank still reaches R.
pub struct Optional<R>(PhantomData<R>);

impl<'a, R: CoerceV2<'a>> CoerceV2<'a> for Optional<R> {
    type Output = Option<R::Output>;

    fn coercev2_input(
        input: Input<'a>,
        context: &mut CoercionV2Context,
    ) -> Result<Self::Output, CoercionV2Failure> {
        if input.value().is_none() {
            Ok(None)
        } else {
            coercev2::<R>(input, context).map(Some)
        }
    }
}

macro_rules! origin_rule {
    ($name:ident, $origin:ident, $description:literal) => {
        #[doc = $description]
        pub struct $name<R>(PhantomData<R>);

        impl<'a, R: CoerceV2<'a>> CoerceV2<'a> for $name<R> {
            type Output = R::Output;

            fn coercev2_input(
                input: Input<'a>,
                context: &mut CoercionV2Context,
            ) -> Result<Self::Output, CoercionV2Failure> {
                if input.origin() != Origin::$origin {
                    return Err(CoercionV2Failure::mismatch($description));
                }
                coercev2::<R>(input, context)
            }
        }
    };
}

origin_rule!(Direct, Direct, "Apply a rule only to direct scalar inputs.");
origin_rule!(
    Referenced,
    Reference,
    "Apply a rule only to referenced scalar inputs."
);
origin_rule!(
    ArrayMember,
    Array,
    "Apply a rule only to materialized-array members."
);
