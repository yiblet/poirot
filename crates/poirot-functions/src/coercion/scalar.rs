use crate::coercion::ExpectedInput;
use crate::coercion::{
    CellError, CellValue, Coerce, CoercionContext, CoercionError, Input, Origin, TextRef, coerce,
    parsing::{coerce_decimal_text, coerce_text_view},
};
use std::{borrow::Cow, marker::PhantomData};

fn required_value(input: Input<'_>) -> Result<&CellValue, CoercionError> {
    match input.value() {
        Some(CellValue::Error(error)) => Err(CoercionError::Worksheet(*error)),
        Some(value) => Ok(value),
        None => Err(CoercionError::mismatch(ExpectedInput::PresentScalar)),
    }
}

/// Require an existing number, without changing its IEEE-754 payload.
/// Finiteness/domain validation belongs to a consuming rule, not type inspection.
pub struct ExactNumber;

impl Coerce<'_> for ExactNumber {
    type Output = f64;

    fn coerce_input(input: Input<'_>, _: &mut CoercionContext) -> Result<f64, CoercionError> {
        match required_value(input)? {
            CellValue::Number(number) => Ok(*number),
            _ => Err(CoercionError::mismatch(ExpectedInput::ExistingNumber)),
        }
    }
}

/// Require an existing boolean; do not interpret numbers or text.
pub struct ExactBoolean;

impl Coerce<'_> for ExactBoolean {
    type Output = bool;

    fn coerce_input(input: Input<'_>, _: &mut CoercionContext) -> Result<bool, CoercionError> {
        match required_value(input)? {
            CellValue::Boolean(boolean) => Ok(*boolean),
            _ => Err(CoercionError::mismatch(ExpectedInput::ExistingBoolean)),
        }
    }
}

/// Borrow exact text, retaining isolated UTF-16 surrogates without allocation.
pub struct ExactText;

impl<'a> Coerce<'a> for ExactText {
    type Output = TextRef<'a>;

    fn coerce_input(
        input: Input<'a>,
        _: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError> {
        required_value(input)?
            .text_ref()
            .ok_or_else(|| CoercionError::mismatch(ExpectedInput::ExistingText))
    }
}

/// Explicitly request a worksheet error as data, rather than propagating it.
pub struct ExactError;

impl Coerce<'_> for ExactError {
    type Output = CellError;

    fn coerce_input(input: Input<'_>, _: &mut CoercionContext) -> Result<CellError, CoercionError> {
        match input.value() {
            Some(CellValue::Error(error)) => Ok(*error),
            _ => Err(CoercionError::mismatch(
                ExpectedInput::ExistingWorksheetError,
            )),
        }
    }
}

/// Preserve scalar type, omission, error value, and origin for a selector/predicate.
pub struct Identity;

impl<'a> Coerce<'a> for Identity {
    type Output = Input<'a>;

    fn coerce_input(input: Input<'a>, _: &mut CoercionContext) -> Result<Input<'a>, CoercionError> {
        Ok(input)
    }
}

/// Finite number, blank-as-zero, boolean-as-zero/one, or finite decimal text.
/// This is an explicit decimal-only policy, not full Excel arithmetic or VALUE.
pub struct DecimalNumber;

impl Coerce<'_> for DecimalNumber {
    type Output = f64;

    fn coerce_input(input: Input<'_>, context: &mut CoercionContext) -> Result<f64, CoercionError> {
        let value = required_value(input)?;
        match value {
            CellValue::Number(number) if number.is_finite() => Ok(*number),
            CellValue::Number(_) => Err(CoercionError::Worksheet(CellError::Num)),
            CellValue::Blank => Ok(0.0),
            CellValue::Boolean(boolean) => Ok(f64::from(u8::from(*boolean))),
            _ => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionError::mismatch(ExpectedInput::FiniteDecimalNumber))?;
                let text = coerce_text_view(text, context)?;
                coerce_decimal_text(&text)
                    .ok_or_else(|| CoercionError::mismatch(ExpectedInput::FiniteDecimalNumber))
            }
        }
    }
}

/// Boolean, finite numeric truthiness, blank-as-false, or exact ASCII TRUE/FALSE
/// spelling (case-insensitive, with no trimming). This is not AND/OR collection policy.
pub struct LogicalBoolean;

impl Coerce<'_> for LogicalBoolean {
    type Output = bool;

    fn coerce_input(
        input: Input<'_>,
        context: &mut CoercionContext,
    ) -> Result<bool, CoercionError> {
        match required_value(input)? {
            CellValue::Boolean(boolean) => Ok(*boolean),
            CellValue::Blank => Ok(false),
            CellValue::Number(number) if number.is_finite() => Ok(*number != 0.0),
            CellValue::Number(_) => Err(CoercionError::Worksheet(CellError::Num)),
            value => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionError::mismatch(ExpectedInput::LogicalScalar))?;
                let length = u64::try_from(text.storage_bytes())
                    .map_err(|_| CoercionError::Limit(crate::coercion::Resource::Work))?;
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
                    Err(CoercionError::mismatch(ExpectedInput::LogicalScalar))
                }
            }
        }
    }
}

/// Unicode scalar formula text; no cell display format is consulted.
/// Use ExactText when exact UTF-16 units, including isolated surrogates, are required.
pub struct FormulaText;

impl<'a> Coerce<'a> for FormulaText {
    type Output = Cow<'a, str>;

    fn coerce_input(
        input: Input<'a>,
        context: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError> {
        match required_value(input)? {
            CellValue::Blank => Ok(Cow::Borrowed("")),
            CellValue::Boolean(true) => Ok(Cow::Borrowed("TRUE")),
            CellValue::Boolean(false) => Ok(Cow::Borrowed("FALSE")),
            CellValue::Number(number) if number.is_finite() => {
                context.admit_text(crate::numeric::number_text::ALLOCATION_BOUND)?;
                Ok(Cow::Owned(crate::numeric::number_text::render(*number)))
            }
            CellValue::Number(_) => Err(CoercionError::Worksheet(CellError::Num)),
            value => {
                let text = value
                    .text_ref()
                    .ok_or_else(|| CoercionError::mismatch(ExpectedInput::FormulaText))?;
                coerce_text_view(text, context)
            }
        }
    }
}

/// Nonnegative finite decimal input whose truncated integer fits i32.
/// Functions with other bounds must request a distinct rule.
pub struct TextCount;

impl Coerce<'_> for TextCount {
    type Output = i32;

    fn coerce_input(input: Input<'_>, context: &mut CoercionContext) -> Result<i32, CoercionError> {
        let number = coerce::<DecimalNumber>(input, context)?;
        if number < 0.0 {
            return Err(CoercionError::Worksheet(CellError::Value));
        }
        let number = crate::coercion::compatibility::coerce_text_integer_number(number)
            .map_err(CoercionError::Worksheet)?;
        Ok(number.trunc() as i32)
    }
}

/// Makes a coercion rule optional for an omitted argument slot.
/// Only omitted syntax produces `None`; a referenced blank still reaches `R`.
pub struct Optional<R>(PhantomData<R>);

impl<'a, R: Coerce<'a>> Coerce<'a> for Optional<R> {
    type Output = Option<R::Output>;

    fn coerce_input(
        input: Input<'a>,
        context: &mut CoercionContext,
    ) -> Result<Self::Output, CoercionError> {
        if input.value().is_none() {
            Ok(None)
        } else {
            coerce::<R>(input, context).map(Some)
        }
    }
}

macro_rules! origin_rule {
    ($name:ident, $origin:ident, $description:literal) => {
        #[doc = $description]
        /// Provenance is checked before the inner rule runs, so a mismatch cannot
        /// trigger its conversions or resource charges. The origin itself is preserved.
        pub struct $name<R>(PhantomData<R>);

        impl<'a, R: Coerce<'a>> Coerce<'a> for $name<R> {
            type Output = R::Output;

            fn coerce_input(
                input: Input<'a>,
                context: &mut CoercionContext,
            ) -> Result<Self::Output, CoercionError> {
                if input.origin() != Origin::$origin {
                    return Err(CoercionError::mismatch(ExpectedInput::$origin));
                }
                coerce::<R>(input, context)
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
