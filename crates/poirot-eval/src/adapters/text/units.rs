//! Deferred UTF-16 consumers. Every vector/output copy follows admission.
use super::*;
use poirot_functions_text::{java_case, units};
use poirot_model::ExcelText;

fn unsupported(h: &TextHost<'_, '_>, detail: impl std::fmt::Debug) -> TextFailure {
    h.source_fault(&format!("lossless POI text: {detail:?}"))
}
fn plan<T>(h: &TextHost<'_, '_>, result: Result<T, units::PlanFailure>) -> Result<T, TextFailure> {
    result.map_err(|e| match e {
        units::PlanFailure::Value => Failure::Spreadsheet(VALUE_ERROR),
        other => unsupported(h, other),
    })
}
fn reserve(h: &mut TextHost<'_, '_>, r: units::Reservation) -> Result<(), TextFailure> {
    h.check_text(r.text_bytes).map_err(Failure::Operational)?;
    h.check_collection(r.elements)
        .map_err(Failure::Operational)?;
    h.charge_work(r.work).map_err(Failure::Operational)
}
fn rendered(h: &mut TextHost<'_, '_>, s: &ScalarOperand) -> Result<Vec<u16>, TextFailure> {
    if s.missing {
        return Err(h.source_fault("POI string coercion rejects MissingArgEval"));
    }
    if let Some(text) = s.value.text_ref() {
        let len = text.len_units();
        let r = plan(h, units::Reservation::units(len, len))?;
        reserve(h, r)?;
        return Ok(text.units().collect());
    }
    // Numeric renderer owns UTF8 buffer; reserve it before the renderer runs.
    h.check_text(512).map_err(Failure::Operational)?;
    h.charge_work(512).map_err(Failure::Operational)?;
    let utf8 = h.coerce_text_scalar(s, false)?;
    let len = utf8.encode_utf16().count();
    let r = plan(h, units::Reservation::units(len, len))?;
    reserve(h, r)?;
    Ok(utf8.encode_utf16().collect())
}
fn coerce_text_integer(h: &mut TextHost<'_, '_>, index: usize) -> Result<i32, TextFailure> {
    let s = h.single(&index)?;
    h.coerce_text_integer(&s)
}
fn value(output: Vec<u16>) -> CellValue {
    // Do not allocate an implicit UTF8 projection through From<ExcelText>.
    CellValue::TextUtf16(ExcelText::from_units(output))
}
fn copied(h: &mut TextHost<'_, '_>, units: &[u16]) -> Result<CellValue, TextFailure> {
    let r = plan(h, units::Reservation::units(units.len(), units.len()))?;
    reserve(h, r)?;
    Ok(value(units.to_vec()))
}
/// Execute text operations in Excel UTF-16 units without lossy scalar conversion.
pub(crate) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    name: TextFunction,
    arguments: ArgumentsRef<'_>,
    full_case: bool,
) -> EvalResult<Value> {
    let excel = session.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation;
    let paired = session.text_policy.source == TextSourceVersion::ExcelCompatibilityV2Documented;
    let mut h = TextHost {
        host: RefCell::new(CallHost::new(session, sheet, address, arguments)),
        pending: RefCell::new(None),
    };
    let n = arguments.len();
    let valid = match name {
        TextFunction::Len
        | TextFunction::Lower
        | TextFunction::Upper
        | TextFunction::Code
        | TextFunction::Proper => n == 1,
        TextFunction::Left | TextFunction::Right => (1..=2).contains(&n),
        TextFunction::Mid => n == 3,
        TextFunction::Replace => n == 4,
        TextFunction::Find | TextFunction::Search => (2..=3).contains(&n),
        TextFunction::Rept => n == 2,
        TextFunction::Concat | TextFunction::Concatenate => true,
        _ => false,
    };
    if !valid {
        return Ok(err(VALUE_ERROR));
    }
    let result = (|| -> Result<CellValue, TextFailure> {
        if matches!(name, TextFunction::Concat | TextFunction::Concatenate) {
            let mut parts: Vec<Vec<u16>> = Vec::new();
            let mut total = 0usize;
            for index in 0..n {
                let description = if name == TextFunction::Concat {
                    Some(h.host.get_mut().describe(index).map_err(failure)?)
                } else {
                    None
                };
                let (rows, columns) = description
                    .as_ref()
                    .filter(|d| d.is_area)
                    .map_or((1, 1), |d| (d.rows, d.columns));
                let additional = rows
                    .checked_mul(columns)
                    .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
                let elements = parts
                    .len()
                    .checked_add(additional)
                    .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
                h.check_collection(elements).map_err(Failure::Operational)?;
                for row in 0..rows {
                    for column in 0..columns {
                        h.charge_work(1).map_err(Failure::Operational)?;
                        let scalar = if description.as_ref().is_some_and(|d| d.is_area) {
                            h.host
                                .get_mut()
                                .read_cell(index, row, column)
                                .map_err(failure)?
                        } else {
                            h.single(&index)?
                        };
                        let text = rendered(&mut h, &scalar)?;
                        total = total
                            .checked_add(text.len())
                            .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
                        let r = plan(&h, units::Reservation::units(total, 0))?;
                        reserve(&mut h, r)?;
                        parts.push(text);
                    }
                }
            }
            let r = plan(&h, units::Reservation::units(total, total))?;
            reserve(&mut h, r)?;
            let mut output = Vec::with_capacity(total);
            for part in parts {
                output.extend_from_slice(&part);
            }
            return Ok(value(output));
        }
        let first = h.single(&0)?;
        if name == TextFunction::Code {
            if first.missing {
                return Err(h.source_fault("POI string coercion rejects MissingArgEval"));
            }
            let first_unit = if let Some(text) = first.value.text_ref() {
                h.charge_work(1).map_err(Failure::Operational)?;
                text.units().next()
            } else {
                rendered(&mut h, &first)?.first().copied()
            };
            let code = first_unit.ok_or(Failure::Spreadsheet(VALUE_ERROR))?;
            if excel {
                if code > 127 {
                    return Err(h.source_fault(
                        "Excel CODE requires an explicit non-ASCII platform code page",
                    ));
                }
                return Ok(Number(f64::from(code)));
            }
            let bytes = if code >= 10000 {
                5
            } else if code >= 1000 {
                4
            } else if code >= 100 {
                3
            } else if code >= 10 {
                2
            } else {
                1
            };
            h.check_text(bytes).map_err(Failure::Operational)?;
            h.charge_work(bytes).map_err(Failure::Operational)?;
            return Ok(Text(code.to_string()));
        }
        let text = rendered(&mut h, &first)?;
        let indexed = matches!(
            name,
            TextFunction::Len
                | TextFunction::Left
                | TextFunction::Right
                | TextFunction::Mid
                | TextFunction::Replace
        );
        let length = if paired && indexed {
            // Length and up to two boundary scans; no offset vector allocation.
            let work = text
                .len()
                .checked_mul(3)
                .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
            h.charge_work(work).map_err(Failure::Operational)?;
            units::paired_length(&text)
        } else {
            text.len()
        };
        let offsets = |range: std::ops::Range<usize>| {
            if paired {
                units::paired_offset(&text, range.start)..units::paired_offset(&text, range.end)
            } else {
                range
            }
        };
        match name {
            TextFunction::Len => Ok(Number(length as f64)),
            TextFunction::Proper => {
                if text.iter().any(|unit| *unit > 127) {
                    return Err(h.source_fault("non-ASCII PROPER needs pinned per-UTF16-char category and casing admission"));
                }
                let r = plan(&h, units::Reservation::units(text.len(), text.len()))?;
                reserve(&mut h, r)?;
                let mut output = text;
                let mut uppercase = true;
                for unit in &mut output {
                    let original = *unit as u8;
                    *unit = u16::from(if uppercase {
                        original.to_ascii_uppercase()
                    } else {
                        original.to_ascii_lowercase()
                    });
                    uppercase = !original.is_ascii_alphabetic();
                }
                Ok(value(output))
            }
            TextFunction::Left | TextFunction::Right => {
                let count = if n == 2 {
                    coerce_text_integer(&mut h, 1)?
                } else {
                    1
                };
                let range = plan(
                    &h,
                    if name == TextFunction::Left {
                        units::left(length, count)
                    } else {
                        units::right(length, count)
                    },
                )?;
                copied(
                    &mut h,
                    text.get(offsets(range))
                        .ok_or(Failure::Spreadsheet(VALUE_ERROR))?,
                )
            }
            TextFunction::Mid => {
                let start = coerce_text_integer(&mut h, 1)?;
                let count = coerce_text_integer(&mut h, 2)?;
                let range = plan(
                    &h,
                    if paired {
                        units::paired_mid(length, start, count)
                    } else {
                        units::mid(length, start, count)
                    },
                )?;
                copied(
                    &mut h,
                    text.get(offsets(range))
                        .ok_or(Failure::Spreadsheet(VALUE_ERROR))?,
                )
            }
            TextFunction::Replace => {
                let start = coerce_text_integer(&mut h, 1)?;
                let count = coerce_text_integer(&mut h, 2)?;
                let removed = offsets(plan(&h, units::replace(length, start, count))?);
                let replacement = h.single(&3)?;
                let replacement = rendered(&mut h, &replacement)?;
                let size = text
                    .len()
                    .checked_sub(removed.len())
                    .and_then(|n| n.checked_add(replacement.len()))
                    .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
                let reservation = plan(&h, units::Reservation::units(size, size))?;
                reserve(&mut h, reservation)?;
                let mut output = Vec::with_capacity(size);
                output.extend_from_slice(
                    text.get(..removed.start)
                        .ok_or(Failure::Spreadsheet(VALUE_ERROR))?,
                );
                output.extend_from_slice(&replacement);
                output.extend_from_slice(
                    text.get(removed.end..)
                        .ok_or(Failure::Spreadsheet(VALUE_ERROR))?,
                );
                Ok(value(output))
            }
            TextFunction::Lower | TextFunction::Upper => {
                if !full_case && text.iter().any(|u| *u > 127) {
                    return Err(
                        h.source_fault("non-ASCII casing requires Poi7321209OpenJdk21Unicode15")
                    );
                }
                let r = plan(&h, units::case_reservation(text.len()))?;
                reserve(&mut h, r)?;
                let direction = if name == TextFunction::Lower {
                    java_case::Direction::Lower
                } else {
                    java_case::Direction::Upper
                };
                let output =
                    java_case::convert(&text, direction).map_err(|e| unsupported(&h, e))?;
                Ok(value(output))
            }
            TextFunction::Find | TextFunction::Search => {
                let second = h.single(&1)?;
                let hay = rendered(&mut h, &second)?;
                let start = if n == 3 {
                    coerce_text_integer(&mut h, 2)?
                } else {
                    1
                };
                let start = plan(&h, units::find_start(start))?;
                let position = if name == TextFunction::Search {
                    if !full_case && hay.iter().chain(&text).any(|u| *u > 127) {
                        return Err(h.source_fault(
                            "non-ASCII SEARCH requires Poi7321209OpenJdk21Unicode15",
                        ));
                    }
                    let r = plan(&h, units::search_reservation(hay.len(), text.len()))?;
                    reserve(&mut h, r)?;
                    java_case::search(&hay, &text, start).map_err(|e| unsupported(&h, e))?
                } else {
                    let work = hay
                        .len()
                        .checked_mul(text.len())
                        .ok_or_else(|| unsupported(&h, units::PlanFailure::SizeOverflow))?;
                    h.charge_work(work).map_err(Failure::Operational)?;
                    java_case::index_of(&hay, &text, start)
                };
                Ok(Number(
                    position.ok_or(Failure::Spreadsheet(VALUE_ERROR))? as f64 + 1.,
                ))
            }
            TextFunction::Rept => {
                // REPT masks only semantic errors in its second argument.
                let second = h.single(&1).map_err(|e| match e {
                    Failure::Spreadsheet(_) => Failure::Spreadsheet(VALUE_ERROR),
                    other => other,
                })?;
                if second.missing {
                    return Err(h.source_fault("REPT numeric MissingArgEval"));
                }
                let count = match &second.value {
                    Text(t) => h
                        .host
                        .get_mut()
                        .coerce_number_text(t, NumericPolicy::PinnedPoi)
                        .map_err(failure),
                    v => h
                        .host
                        .get_mut()
                        .coerce_scalar_number(v, NumericPolicy::PinnedPoi)
                        .map_err(failure),
                }
                .map_err(|e| match e {
                    Failure::Spreadsheet(_) => Failure::Spreadsheet(VALUE_ERROR),
                    other => other,
                })?;
                let (count, r) = plan(&h, units::rept(text.len(), count))?;
                reserve(&mut h, r)?;
                let mut output = Vec::with_capacity(r.elements);
                // Empty repeats can skip source loop after accounting its work.
                if !text.is_empty() {
                    for _ in 0..count {
                        output.extend_from_slice(&text);
                    }
                }
                Ok(value(output))
            }
            _ => Err(Failure::Spreadsheet(VALUE_ERROR)),
        }
    })();
    match result {
        Ok(v) => Ok(v.into()),
        Err(Failure::Spreadsheet(e)) => Ok(err(e)),
        Err(Failure::Operational(e)) => Err(e),
        Err(other) => Err(h.host.get_mut().capability_failure(
            CapabilityId::FunctionAdapter,
            format!("lossless text: {other:?}"),
        )),
    }
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
