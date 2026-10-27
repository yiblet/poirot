//! Lossless text kernels use shared typed broadcast and collection traversal.

use crate::eval::adapters::elementwise::{
    self, Broadcast, Kernel, MemberContext, Variadic, WholeCollection, collections, text as shared,
};
use crate::eval::functions::registry::{FunctionId, TextFunction};
use crate::eval::host::CallHost;
use crate::eval::limits::{CapabilityId, EvalResult};
use crate::eval::values::{FunctionResult, VALUE_ERROR, Value, err};
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellValue};
use poirot_functions::text::{java_case, units};

/// Runs UTF-16 text operations for each broadcast member.
/// Ordinary operations count surrogate pairs together; non-DBCS B functions count
/// each UTF-16 unit separately through the same kernels.
struct TextKernel {
    name: TextFunction,
    paired: bool,
}

impl Kernel for TextKernel {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        arguments: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let name = self.name;
        let text = arguments.text_units(context, 0)?;
        let indexed = matches!(
            name,
            TextFunction::Len
                | TextFunction::Left
                | TextFunction::Right
                | TextFunction::Mid
                | TextFunction::Replace
        );
        let length = if indexed && self.paired {
            units::paired_length(&text)
        } else {
            text.len()
        };
        let offsets = |range: std::ops::Range<usize>| {
            if self.paired {
                units::paired_offset(&text, range.start)..units::paired_offset(&text, range.end)
            } else {
                range
            }
        };
        match name {
            TextFunction::Len => Ok(Number(length as f64)),
            TextFunction::Code => {
                let code = text.first().copied().ok_or(VALUE_ERROR)?;
                if code > 127 {
                    return Err(context.host.unsupported(
                        CapabilityId::FunctionAdapter,
                        crate::eval::limits::EvaluationReasonError::ExcelCodeRequiresAnExplicitNonAsciiPlatformCodePage,
                    ));
                }
                Ok(Number(f64::from(code)))
            }
            TextFunction::Proper | TextFunction::Lower | TextFunction::Upper => {
                let reservation = shared::plan(context.host, units::case_reservation(text.len()))?;
                shared::reserve(context.host, reservation)?;
                Ok(shared::value(if name == TextFunction::Proper {
                    units::proper(&text)
                } else {
                    units::case(&text, name == TextFunction::Upper)
                }))
            }
            TextFunction::Left | TextFunction::Right | TextFunction::Mid => {
                let (start, count) = if name == TextFunction::Mid {
                    (
                        arguments.text_integer(context, 1, None)?,
                        arguments.text_integer(context, 2, None)?,
                    )
                } else {
                    (0, arguments.text_integer(context, 1, Some(1))?)
                };
                let range = shared::plan(
                    context.host,
                    match name {
                        TextFunction::Left => units::left(length, count),
                        TextFunction::Right => units::right(length, count),
                        _ => units::mid(length, start, count),
                    },
                )?;
                shared::copied(context.host, text.get(offsets(range)).ok_or(VALUE_ERROR)?)
            }
            TextFunction::Replace => {
                let start = arguments.text_integer(context, 1, None)?;
                let count = arguments.text_integer(context, 2, None)?;
                let removed = offsets(shared::plan(
                    context.host,
                    units::replace(length, start, count),
                )?);
                let replacement = arguments.text_units(context, 3)?;
                let size = text
                    .len()
                    .checked_sub(removed.len())
                    .and_then(|n| n.checked_add(replacement.len()))
                    .ok_or_else(|| context.host.size_overflow())?;
                let reservation =
                    shared::plan(context.host, units::Reservation::units(size, size))?;
                shared::reserve(context.host, reservation)?;
                let mut output = Vec::new();
                output
                    .try_reserve_exact(size)
                    .map_err(|reservation_source| {
                        context.host.reservation_failure(reservation_source)
                    })?;
                // SAFETY: size is the checked sum of retained prefixes, replacement, and suffix.
                output.extend_from_slice(text.get(..removed.start).ok_or(VALUE_ERROR)?);
                // SAFETY: the local size reservation includes every replacement unit.
                output.extend_from_slice(&replacement);
                // SAFETY: the local size reservation includes the complete retained suffix.
                output.extend_from_slice(text.get(removed.end..).ok_or(VALUE_ERROR)?);
                Ok(shared::value(output))
            }
            TextFunction::Find | TextFunction::Search => {
                let hay = arguments.text_units(context, 1)?;
                let start = arguments.text_integer(context, 2, Some(1))?;
                let hay_length = if self.paired {
                    units::paired_length(&hay)
                } else {
                    hay.len()
                };
                let start = poirot_functions::coercion::compatibility::coerce_text_search_start(
                    f64::from(start),
                    hay_length,
                )?;
                let position = if name == TextFunction::Search {
                    let reservation = shared::plan(
                        context.host,
                        units::wildcard_reservation(hay.len(), text.len()),
                    )?;
                    shared::reserve(context.host, reservation)?;
                    units::search(&hay, &text, start, self.paired)
                } else {
                    context
                        .host
                        .charge_work(hay.len().saturating_mul(text.len()) as u64)?;
                    let offset = if self.paired {
                        units::paired_offset(&hay, start)
                    } else {
                        start
                    };
                    java_case::index_of(&hay, &text, offset).map(|offset| {
                        if self.paired {
                            units::paired_length(hay.get(..offset).unwrap_or_default())
                        } else {
                            offset
                        }
                    })
                };
                Ok(Number(position.ok_or(VALUE_ERROR)? as f64 + 1.))
            }
            TextFunction::Rept => {
                let count = arguments.text_integer(context, 1, Some(0))?;
                let (count, mut reservation) =
                    shared::plan(context.host, units::rept(text.len(), f64::from(count)))?;
                if text.is_empty() {
                    reservation.work = 0;
                }
                shared::reserve(context.host, reservation)?;
                let mut output = Vec::new();
                output
                    .try_reserve_exact(reservation.elements)
                    .map_err(|reservation_source| {
                        context.host.reservation_failure(reservation_source)
                    })?;
                if !text.is_empty() {
                    for _ in 0..count {
                        output
                            .try_reserve(text.len())
                            .map_err(|reservation_source| {
                                context.host.reservation_failure(reservation_source)
                            })?;
                        // SAFETY: the complete source slice was reserved immediately above.
                        output.extend_from_slice(&text);
                    }
                }
                Ok(shared::value(output))
            }
            _ => Err(VALUE_ERROR.into()),
        }
    }
}

/// CONCAT kernel that folds ordered source collections into one UTF-16 result.
/// Shared traversal preserves read order and stops when a consumed operand fails.
struct ConcatKernel;

impl collections::CollectionKernel for ConcatKernel {
    type Signature = Variadic<WholeCollection>;

    fn apply(
        &self,
        host: &mut CallHost<'_, '_>,
        arguments: &Self::Signature,
        _address: CellAddress,
    ) -> FunctionResult<Value> {
        // Absent cells concatenate as empty text, so only stored cells are read.
        let output = collections::fold_stored(
            host,
            arguments.iter(),
            Vec::<u16>::new(),
            |_, _, _| Ok(()),
            |host, output, operand| {
                let text = shared::units(host, &operand)?;
                let size = output
                    .len()
                    .checked_add(text.len())
                    .ok_or_else(|| host.size_overflow())?;
                let reservation = shared::plan(host, units::Reservation::units(size, text.len()))?;
                shared::reserve(host, reservation)?;
                output
                    .try_reserve(text.len())
                    .map_err(|reservation_source| host.reservation_failure(reservation_source))?;
                // SAFETY: text.len() additional slots were reserved immediately above.
                output.extend_from_slice(&text);
                Ok(())
            },
        )?;
        Ok(shared::value(output).into())
    }
}

/// Dispatch ordinary text or legacy B functions with their respective UTF-16 indexing.
pub(in crate::eval) fn execute(
    mut host: CallHost<'_, '_>,
    name: TextFunction,
    length: usize,
) -> EvalResult<Value> {
    if !FunctionId::Text(name).descriptor().arity.accepts(length) {
        return Ok(err(VALUE_ERROR));
    }
    // Microsoft MS-OE376 LENB/FINDB/MIDB notes distinguish non-DBCS default
    // languages from DBCS languages; non-DBCS positions count legacy UTF-16
    // units, independently of ordinary text Compatibility Version 2 indexing.
    // https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/7a8bce94-30cb-44a6-a2a5-e4b2ac26eab3
    // https://support.microsoft.com/en-us/excel/compatibility-versions
    // Only the non-DBCS contract is verified here: do not infer Excel's DBCS
    // classification, split-byte padding, or offsets from UTF-8/codepage length.
    let byte_operation = match name {
        TextFunction::LenB => Some(TextFunction::Len),
        TextFunction::LeftB => Some(TextFunction::Left),
        TextFunction::RightB => Some(TextFunction::Right),
        TextFunction::MidB => Some(TextFunction::Mid),
        TextFunction::FindB => Some(TextFunction::Find),
        TextFunction::SearchB => Some(TextFunction::Search),
        TextFunction::ReplaceB => Some(TextFunction::Replace),
        _ => None,
    };
    if byte_operation.is_some()
        && host.byte_text_language() != crate::eval::config::ByteTextLanguage::NonDbcs
    {
        return Err(host.capability_failure(
            CapabilityId::FunctionAdapter,
            crate::eval::limits::EvaluationReasonError::DbcsByteText,
        ));
    }
    let address = host.target().address;
    let result = if name == TextFunction::Concat {
        collections::execute(&mut host, address, ConcatKernel)
    } else {
        elementwise::execute(
            &mut host,
            address,
            TextKernel {
                name: byte_operation.unwrap_or(name),
                paired: byte_operation.is_none(),
            },
        )
    };
    super::outcome::finish(result)
}
