use super::formatting::text_format;
use super::operator_units;
use crate::eval::adapters::elementwise::{self};
use crate::eval::functions::registry::{FunctionId, TextFunction};
use crate::eval::host::{ArgumentCoercion, CallHost};
use crate::eval::limits::{CapabilityId, EvaluationError};
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR};
use crate::model::CellValue;
use crate::model::CellValue::{Blank, Boolean, Error, Number, Text};

/// Evaluate eager scalar text operations in source order.
pub(super) fn text_scalar(
    host: &mut CallHost<'_, '_>,
    name: TextFunction,
    a: &[CellValue],
) -> FunctionResult<CellValue> {
    let n = a.len();
    if !FunctionId::Text(name).descriptor().arity.accepts(n) {
        return Err(VALUE_ERROR.into());
    }

    if name == TextFunction::Concatenate {
        let mut result = Vec::new();
        for value in a {
            let text = ArgumentCoercion::new(host).coerce_text_units(value)?;
            let size = result
                .len()
                .checked_add(text.len())
                .ok_or_else(|| host.size_overflow())?;
            if size > 32767 {
                return Err(VALUE_ERROR.into());
            }
            host.check_text_capacity(size.saturating_mul(2))?;
            host.charge_work(text.len() as u64)?;
            result
                .try_reserve(text.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            // SAFETY: text.len() additional slots were reserved immediately above.
            result.extend_from_slice(&text);
        }
        return Ok(elementwise::text::value(result));
    }
    if name == TextFunction::Text {
        return text_format(host, a);
    }
    if name == TextFunction::Arabic {
        let value = a.first().ok_or(VALUE_ERROR)?;
        let view = poirot_functions::coercion::roman::coerce_arabic_scalar(value.into())?;
        let text = ArgumentCoercion::new(host).scalar_text_view(view)?;
        return Ok(Number(f64::from(
            poirot_functions::coercion::roman::coerce_arabic_text(&text)?,
        )));
    }
    if name == TextFunction::Roman {
        let number = ArgumentCoercion::new(host)
            .coerce_scalar_number(a.first().ok_or(VALUE_ERROR)?)?
            .floor();
        if !(0.0..=3999.0).contains(&number) {
            return Err(VALUE_ERROR.into());
        }
        let form = match a.get(1) {
            None | Some(Blank) => 0,
            Some(Boolean(value)) => {
                if *value {
                    0
                } else {
                    4
                }
            }
            Some(value) => ArgumentCoercion::new(host)
                .coerce_scalar_number(value)?
                .floor() as i32,
        };
        if !(0..=4).contains(&form) {
            return Err(VALUE_ERROR.into());
        }
        host.check_text(16)?;
        return Ok(Text(poirot_functions::text::roman(
            number as u16,
            form as u8,
        )));
    }
    if matches!(name, TextFunction::Char | TextFunction::UniChar) {
        let code = elementwise::text::integer_argument(host, a, 0)?;
        if code < 1.
            || code
                > if name == TextFunction::Char {
                    255.
                } else {
                    1114111.
                }
        {
            return Err(VALUE_ERROR.into());
        }
        let character = char::from_u32(code as u32).ok_or(VALUE_ERROR)?;
        host.check_text(character.len_utf8())?;
        return Ok(Text(character.to_string()));
    }
    if name == TextFunction::Value {
        let value = a.first().ok_or(VALUE_ERROR)?;
        let number =
            poirot_functions::coercion::compatibility::coerce_value_number(value.into(), |text| {
                let text = ArgumentCoercion::new(host).scalar_text_view(text)?;
                match ArgumentCoercion::new(host).formatted_number_text(&text)? {
                    poirot_functions::coercion::compatibility::NumericTextDecision::Number(
                        result,
                    ) => result.map_err(FunctionError::from),
                    poirot_functions::coercion::compatibility::NumericTextDecision::DateTime => {
                        ArgumentCoercion::new(host).legacy_numeric_date_time(&text)
                    }
                }
            })?;
        return Ok(Number(number));
    }

    if matches!(name, TextFunction::Asc | TextFunction::Dbcs) {
        let input = ArgumentCoercion::new(host).coerce_text_units(a.first().ok_or(VALUE_ERROR)?)?;
        let mode = match name {
            TextFunction::Asc => poirot_functions::text::width::WidthConversion::Narrow,
            _ => poirot_functions::text::width::WidthConversion::Wide,
        };
        host.charge_work(input.len() as u64)?;
        let units = mode.units(&input);
        let count = units.clone().take(32768).count();
        if count > 32767 {
            return Err(VALUE_ERROR.into());
        }
        host.check_text_capacity(count.saturating_mul(2))?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(count)
            .map_err(|_| host.allocation_failure())?;
        for unit in units {
            // SAFETY: count measured this identical iterator, and was reserved above.
            output.push(unit);
        }
        return Ok(elementwise::text::value(output));
    }

    let t = elementwise::text::argument(host, a, 0)?;
    host.check_text(t.len())?;
    host.check_values(t.chars().count())?;

    Ok(match name {
        TextFunction::NumberValue
        | TextFunction::Proper
        | TextFunction::Arabic
        | TextFunction::Roman
        | TextFunction::TextJoin => return Err(VALUE_ERROR.into()),
        TextFunction::Trim => {
            let mut result = String::new();
            result
                .try_reserve_exact(t.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            for part in t.split(' ').filter(|s| !s.is_empty()) {
                if !result.is_empty() {
                    // SAFETY: Collapsing spaces and omitting edge spaces cannot exceed t.len().
                    result.push(' ');
                }
                // SAFETY: Selected parts and collapsed spaces cannot exceed t.len(), reserved locally.
                result.push_str(part);
            }
            Text(result)
        }
        TextFunction::Clean => {
            let mut result = String::new();
            result
                .try_reserve_exact(t.len())
                .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
            for c in t.chars().filter(|c| (*c as u32) >= 32) {
                // SAFETY: Removing characters cannot exceed the locally reserved input byte length.
                result.push(c);
            }
            Text(result)
        }
        TextFunction::EncodeUrl => {
            let encoded = poirot_functions::text::url::encode_url(&t);
            // Each input byte produces at most three ASCII characters. Reject
            // Excel's cell-text limit during measurement before reserving output.
            let bytes = encoded
                .clone()
                .try_fold(0usize, |bytes, character| {
                    bytes
                        .checked_add(character.len_utf8())
                        .filter(|size| *size <= 32767)
                })
                .ok_or(VALUE_ERROR)?;
            host.check_text(bytes)?;
            let mut result = String::new();
            result
                .try_reserve_exact(bytes)
                .map_err(|_| host.allocation_failure())?;
            for character in encoded {
                // SAFETY: bytes sums the UTF-8 lengths of this cloned iterator, reserved above.
                result.push(character);
            }
            Text(result)
        }
        TextFunction::Unicode => Number(t.chars().next().ok_or(VALUE_ERROR)? as u32 as f64),
        TextFunction::Exact => Boolean(t == elementwise::text::argument(host, a, 1)?),
        TextFunction::Substitute => {
            let old = elementwise::text::argument(host, a, 1)?;
            let new = elementwise::text::argument(host, a, 2)?;
            if old.is_empty() {
                Text(t)
            } else {
                let nth = if n == 3 {
                    None
                } else {
                    let nth = elementwise::text::integer_argument(host, a, 3)?;
                    if nth < 1. {
                        return Err(VALUE_ERROR.into());
                    }
                    Some(nth as usize)
                };
                host.check_text(t.len())?;
                let matches = t.match_indices(&old).count();
                let replaced = nth.map_or(matches, |nth| usize::from(nth <= matches));
                let bytes = replaced
                    .checked_mul(old.len())
                    .and_then(|removed| t.len().checked_sub(removed))
                    .and_then(|remaining| {
                        replaced
                            .checked_mul(new.len())
                            .and_then(|added| remaining.checked_add(added))
                    })
                    .ok_or_else(|| host.size_overflow())?;
                host.check_text(bytes)?;
                {
                    let mut result = String::new();
                    result
                        .try_reserve_exact(bytes)
                        .map_err(|reservation_source| {
                            host.reservation_failure(reservation_source)
                        })?;
                    let mut occurrence = 0;
                    for part in t.split_inclusive(&old) {
                        if let Some(prefix) = part.strip_suffix(&old) {
                            occurrence += 1;
                            // SAFETY: bytes is the checked final replacement length, reserved locally.
                            result.push_str(prefix);
                            // SAFETY: bytes accounts for all replacements selected by nth.
                            result.push_str(if nth.is_none() || Some(occurrence) == nth {
                                &new
                            } else {
                                &old
                            });
                        } else {
                            // SAFETY: bytes includes the unchanged final suffix.
                            result.push_str(part);
                        }
                    }
                    Text(result)
                }
            }
        }
        TextFunction::LenB
        | TextFunction::LeftB
        | TextFunction::RightB
        | TextFunction::MidB
        | TextFunction::FindB
        | TextFunction::SearchB
        | TextFunction::ReplaceB
        | TextFunction::Asc
        | TextFunction::Dbcs
        | TextFunction::FilterXml
        | TextFunction::BahtText
        | TextFunction::Text
        | TextFunction::Char
        | TextFunction::UniChar
        | TextFunction::Value
        | TextFunction::Concat
        | TextFunction::Concatenate
        | TextFunction::Len
        | TextFunction::Left
        | TextFunction::Right
        | TextFunction::Mid
        | TextFunction::Replace
        | TextFunction::Find
        | TextFunction::Search
        | TextFunction::Lower
        | TextFunction::Upper
        | TextFunction::Rept
        | TextFunction::Code
        | TextFunction::TextBefore
        | TextFunction::TextAfter
        | TextFunction::TextSplit
        | TextFunction::ArrayToText
        | TextFunction::ValueToText
        | TextFunction::RegexTest
        | TextFunction::RegexExtract
        | TextFunction::RegexReplace => return Err(VALUE_ERROR.into()),
    })
}

pub(super) fn text_unsupported(
    reason: poirot_functions::text_units::TextSemanticsError,
) -> FunctionError {
    EvaluationError::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        target: None,
        reason: crate::eval::limits::EvaluationReasonError::TextSemantics(std::sync::Arc::new(
            reason,
        )),
    }
    .into()
}

/// Execute concatenation with selected text-unit semantics and metered storage.
pub(in crate::eval) fn concat_operator(
    host: &mut CallHost<'_, '_>,
    left: CellValue,
    right: CellValue,
) -> FunctionResult<CellValue> {
    if let Error(error) = left {
        return Ok(Error(error));
    }
    if let Error(error) = right {
        return Ok(Error(error));
    }

    operator_units::text_unit_operator(host, &left, &right)
}
