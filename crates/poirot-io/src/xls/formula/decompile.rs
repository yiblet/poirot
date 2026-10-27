//! BIFF token reconstruction and reference rendering.
use super::add_ins::{AddIns, arity};
use super::array_constant::array_constant;
use super::metadata::function_metadata;
use super::parsed_formula::ParsedFormula;
use crate::xls::preflight::{PreflightError, ResourceBudget};
use crate::xls::result::Result;
use crate::xls::text::{take, text, word};
use crate::{CellAddress, NamedRange};

fn reference(row: u16, col: u16) -> String {
    let mut n = (col & 0x3fff) as u32 + 1;
    let mut letters = Vec::new();
    while n > 0 {
        n -= 1;
        letters.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    let column: String = letters.into_iter().rev().collect();
    format!(
        "{}{}{}{}",
        if col & 0x4000 == 0 { "$" } else { "" },
        column,
        if col & 0x8000 == 0 { "$" } else { "" },
        row as u32 + 1
    )
}

/// Resolve a PtgRefN/PtgAreaN location against the shared-formula cell.
/// Relative offsets wrap within the BIFF8 grid, matching Excel's storage of
/// negative offsets as 16-bit rows and 8-bit columns (MS-XLS 2.5.198.88).
fn relative(base: CellAddress, row: u16, col: u16) -> (u16, u16) {
    let row = if col & 0x8000 != 0 {
        (base.row as u16).wrapping_add(row)
    } else {
        row
    };
    let column = if col & 0x4000 != 0 {
        ((base.col as u16).wrapping_add(col & 0xff)) & 0xff
    } else {
        col & 0x3fff
    };
    (row, column | (col & 0xc000))
}

/// Render a single-sheet XTI reference prefix, or `None` for external/multi-sheet XTIs.
fn sheet_prefix(xti: u16, sheets: &[String], xtis: &[(u16, u16, u16)]) -> Option<String> {
    let (sup, first, last) = xtis.get(xti as usize)?;
    if *sup != 0 || first != last {
        return None;
    }
    let sheet = sheets.get(*first as usize)?;
    Some(format!("'{}'!", sheet.replace('\'', "''")))
}

/// Spell a BIFF8 BErr error code (MS-XLS 2.5.10).
pub(in crate::xls) fn error_text(code: u8) -> Result<&'static str> {
    Ok(match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0f => "#VALUE!",
        0x17 => "#REF!",
        0x1d => "#NAME?",
        0x24 => "#NUM!",
        0x2a => "#N/A",
        0x2b => "#GETTING_DATA",
        _ => {
            return Err(PreflightError::Malformed(
                crate::BiffStructureError::UnknownBiffErrorCode,
            ));
        }
    })
}

/// Formula reconstruction stack member retaining text and grouping state.
enum Item {
    Expression(String),
    AddIn(String),
}

fn pop(s: &mut Vec<Item>) -> Result<String> {
    match s.pop() {
        Some(Item::Expression(value)) => Ok(value),
        Some(Item::AddIn(_)) => Err(PreflightError::Representation(
            crate::BiffRepresentationError::NamexUsedOutsideExternalFunctionCall,
        )),
        None => Err(PreflightError::Malformed(
            crate::BiffStructureError::FormulaStackUnderflow,
        )),
    }
}

/// Reconstruct supported RPN tokens into formula source without add-in bindings.
#[cfg(test)]
pub(in crate::xls) fn formula(
    b: &[u8],
    sheets: &[String],
    names: &[NamedRange],
    xtis: &[(u16, u16, u16)],
    budget: ResourceBudget,
) -> Result<Option<String>> {
    formula_with_addins(
        ParsedFormula { rgce: b, rgcb: &[] },
        sheets,
        names,
        xtis,
        &AddIns::default(),
        None,
        budget,
    )
}

/// Reconstruct RPN source using checked add-in function identities.
/// `base` anchors relative PtgRefN/PtgAreaN tokens of shared formulas; ordinary
/// cell and name formulas pass `None` and reject those tokens. PtgArray values
/// are read from `tokens.rgcb`.
pub(in crate::xls) fn formula_with_addins(
    tokens: ParsedFormula<'_>,
    sheets: &[String],
    names: &[NamedRange],
    xtis: &[(u16, u16, u16)],
    addins: &AddIns,
    base: Option<CellAddress>,
    budget: ResourceBudget,
) -> Result<Option<String>> {
    let mut b = tokens.rgce;
    if b.is_empty() {
        return Ok(None);
    }
    // RgbExtra holds one entry per PtgArray and PtgMemArea, in token order.
    let mut extra = tokens.rgcb;
    let mut pending_memory_areas = 0usize;
    let mut s = Vec::new();
    let mut token_count = 0usize;
    while !b.is_empty() {
        token_count = token_count
            .checked_add(1)
            .ok_or(PreflightError::Representation(
                crate::BiffRepresentationError::FormulaTokenCountOverflow,
            ))?;
        if token_count > budget.max_metadata_entries {
            return Err(PreflightError::Resource {
                resource: crate::BiffResource::BiffFormulaTokens,
                requested: token_count as u64,
                limit: budget.max_metadata_entries as u64,
            });
        }
        let raw = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
            crate::BiffStructureError::MissingByte,
        ))?;
        let id = if raw >= 0x20 {
            0x20 | (raw & 0x1f)
        } else {
            raw
        };
        match id {
            // Pinned MemArea/MemFunc payloads annotate, but do not consume,
            // the following reference expression. Decode it normally and retain
            // all per-token and stack-text budget checks.
            0x26..=0x29 | 0x2e | 0x2f => {
                if matches!(id, 0x26..=0x28) {
                    take(&mut b, 4)?;
                }
                if id == 0x26 {
                    // PtgMemArea's PtgExtraMem is skipped only when a later
                    // PtgArray needs the extra data after it.
                    pending_memory_areas = pending_memory_areas.saturating_add(1);
                }
                let _subexpression_bytes = word(&mut b)?;
            }
            0x03..=0x11 => {
                let right = pop(&mut s)?;
                let left = pop(&mut s)?;
                // SAFETY: The opcode match bounds id - 3 within the fifteen literal operators; the stack suffix start is checked_sub(count) from the current stack length.
                let op = [
                    "+", "-", "*", "/", "^", "&", "<", "<=", "=", ">", ">=", "<>", " ", ",", ":",
                ][(id - 3) as usize];
                s.push(Item::Expression(format!("{left}{op}{right}")));
            }
            0x12..=0x15 => {
                let e = pop(&mut s)?;
                s.push(Item::Expression(match id {
                    0x12 => format!("+{e}"),
                    0x13 => format!("-{e}"),
                    0x14 => format!("{e}%"),
                    _ => format!("({e})"),
                }));
            }
            0x16 => s.push(Item::Expression(String::new())),
            // PtgArray: seven unused bytes; its values are the next
            // PtgExtraArray in RgbExtra, after any earlier PtgExtraMem
            // (a count of Ref8U ranges, eight bytes each).
            0x20 => {
                take(&mut b, 7)?;
                for _ in 0..pending_memory_areas {
                    let ranges = usize::from(word(&mut extra)?);
                    take(
                        &mut extra,
                        ranges.checked_mul(8).ok_or(PreflightError::Representation(
                            crate::BiffRepresentationError::PtgextramemSizeOverflow,
                        ))?,
                    )?;
                }
                pending_memory_areas = 0;
                s.push(Item::Expression(array_constant(
                    &mut extra,
                    &mut token_count,
                    budget,
                )?));
            }
            0x17 => {
                let n = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))? as usize;
                let t = text(&mut b, n)?;
                s.push(Item::Expression(format!("\"{}\"", t.replace('"', "\"\""))));
            }
            0x19 => {
                let flags = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))?;
                let n = word(&mut b)?;
                if flags & 4 != 0 {
                    take(&mut b, (n as usize + 1) * 2)?;
                }
                if flags & 0x10 != 0 {
                    let e = pop(&mut s)?;
                    s.push(Item::Expression(format!("SUM({e})")));
                }
            }
            0x1d => s.push(Item::Expression(
                if *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))? == 0
                {
                    "FALSE"
                } else {
                    "TRUE"
                }
                .into(),
            )),
            0x1e => s.push(Item::Expression(word(&mut b)?.to_string())),
            0x1f => {
                let v = take(&mut b, 8)?;
                s.push(Item::Expression(
                    f64::from_le_bytes(v.try_into().map_err(|_| {
                        PreflightError::Malformed(crate::BiffStructureError::NumberToken)
                    })?)
                    .to_string(),
                ));
            }
            0x23 => {
                let v = take(&mut b, 4)?;
                let i =
                    u32::from_le_bytes(v.try_into().map_err(|_| {
                        PreflightError::Malformed(crate::BiffStructureError::NameToken)
                    })?) as usize;
                let Some(name) = i.checked_sub(1).and_then(|i| names.get(i)) else {
                    return Ok(None);
                };
                s.push(Item::Expression(name.name.clone()));
            }
            0x24 | 0x25 | 0x2c | 0x2d | 0x3a | 0x3b => {
                let prefix = if id >= 0x3a {
                    let Some(prefix) = sheet_prefix(word(&mut b)?, sheets, xtis) else {
                        return Ok(None);
                    };
                    prefix
                } else {
                    String::new()
                };
                let area = matches!(id, 0x25 | 0x2d | 0x3b);
                let r1 = word(&mut b)?;
                let r2 = if area { word(&mut b)? } else { r1 };
                let c1 = word(&mut b)?;
                let c2 = if area { word(&mut b)? } else { c1 };
                let ((r1, c1), (r2, c2)) = if matches!(id, 0x2c | 0x2d) {
                    let Some(base) = base else {
                        return Ok(None);
                    };
                    (relative(base, r1, c1), relative(base, r2, c2))
                } else {
                    ((r1, c1), (r2, c2))
                };
                let a = reference(r1, c1);
                s.push(Item::Expression(if area {
                    format!("{prefix}{a}:{}", reference(r2, c2))
                } else {
                    format!("{prefix}{a}")
                }));
            }
            // PtgErr carries one BErr code (MS-XLS 2.5.198.52).
            0x1c => {
                let code = *take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MissingByte,
                ))?;
                s.push(Item::Expression(error_text(code)?.into()));
            }
            // PtgRefErr/PtgAreaErr and their 3-D forms keep unused location bytes.
            0x2a | 0x2b | 0x3c | 0x3d => {
                let prefix = if id >= 0x3c {
                    let Some(prefix) = sheet_prefix(word(&mut b)?, sheets, xtis) else {
                        return Ok(None);
                    };
                    prefix
                } else {
                    String::new()
                };
                take(&mut b, if matches!(id, 0x2b | 0x3d) { 8 } else { 4 })?;
                s.push(Item::Expression(format!("{prefix}#REF!")));
            }
            0x39 => {
                let xti = word(&mut b)? as usize;
                let slot = take(&mut b, 4)?;
                let slot = u32::from_le_bytes(slot.try_into().map_err(|_| {
                    PreflightError::Malformed(crate::BiffStructureError::NamexSlot)
                })?);
                let xti = *xtis.get(xti).ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::NamexXti,
                ))?;
                let name = addins.resolve(xti, slot)?;
                if name.len() > budget.max_string_bytes {
                    return Err(PreflightError::Resource {
                        resource: crate::BiffResource::BiffFormulaBytes,
                        requested: name.len() as u64,
                        limit: budget.max_string_bytes as u64,
                    });
                }
                s.push(Item::AddIn(name.to_owned()));
            }
            0x21 | 0x22 => {
                let count = if id == 0x22 {
                    Some(*take(&mut b, 1)?.first().ok_or(PreflightError::Malformed(
                        crate::BiffStructureError::MissingByte,
                    ))? as usize)
                } else {
                    None
                };
                let index = word(&mut b)?;
                if index == 255 {
                    let count = count.ok_or(PreflightError::Malformed(
                        crate::BiffStructureError::ExternalFunctionMustUseFuncvar,
                    ))?;
                    let start = s.len().checked_sub(count).ok_or(PreflightError::Malformed(
                        crate::BiffStructureError::ExternalFunctionStack,
                    ))?;
                    let Some(Item::AddIn(name)) = s.get(start) else {
                        return Err(PreflightError::Representation(
                            crate::BiffRepresentationError::UdfFunctionIdentityUnsupported,
                        ));
                    };
                    let (min, max) = arity(name).ok_or(PreflightError::Representation(
                        crate::BiffRepresentationError::UnsupportedAddInFunctionIdentity,
                    ))?;
                    if count == 0 || count - 1 < min || count - 1 > max {
                        return Err(PreflightError::Malformed(
                            crate::BiffStructureError::ExternalFunctionArity,
                        ));
                    }
                    // SAFETY: The opcode match bounds id - 3 within the fifteen literal operators; the stack suffix start is checked_sub(count) from the current stack length.
                    let bytes = s[start..]
                        .iter()
                        .try_fold(
                            count.checked_add(1).ok_or(PreflightError::Representation(
                                crate::BiffRepresentationError::ExternalFormulaSizeOverflow,
                            ))?,
                            |total, item| {
                                total.checked_add(match item {
                                    Item::Expression(text) | Item::AddIn(text) => text.len(),
                                })
                            },
                        )
                        .ok_or(PreflightError::Representation(
                            crate::BiffRepresentationError::ExternalFormulaSizeOverflow,
                        ))?;
                    // Includes name, arguments, commas and parentheses before split/join/format allocations.
                    if bytes > budget.max_string_bytes {
                        return Err(PreflightError::Resource {
                            resource: crate::BiffResource::BiffFormulaBytes,
                            requested: bytes as u64,
                            limit: budget.max_string_bytes as u64,
                        });
                    }
                    // SAFETY: start is computed by checked subtraction from s.len(), without intervening mutation of s.
                    let mut items = s.split_off(start).into_iter();
                    let Some(Item::AddIn(name)) = items.next() else {
                        return Err(PreflightError::Malformed(
                            crate::BiffStructureError::ExternalFunctionIdentity,
                        ));
                    };
                    let args = items
                        .map(|item| match item {
                            Item::Expression(value) => Ok(value),
                            Item::AddIn(_) => Err(PreflightError::Malformed(
                                crate::BiffStructureError::NestedExternalFunctionIdentity,
                            )),
                        })
                        .collect::<Result<Vec<_>>>()?;
                    s.push(Item::Expression(format!("{name}({})", args.join(","))));
                    let retained = s
                        .iter()
                        .try_fold(0usize, |total, item| {
                            total.checked_add(match item {
                                Item::Expression(text) | Item::AddIn(text) => text.len(),
                            })
                        })
                        .ok_or(PreflightError::Representation(
                            crate::BiffRepresentationError::FormulaTextOverflow,
                        ))?;
                    if retained > budget.max_string_bytes {
                        return Err(PreflightError::Resource {
                            resource: crate::BiffResource::BiffFormulaBytes,
                            requested: retained as u64,
                            limit: budget.max_string_bytes as u64,
                        });
                    }
                    continue;
                }
                let Some((name, min, max)) = function_metadata(index) else {
                    return Ok(None);
                };
                let n = if let Some(n) = count {
                    if n < min || n > max {
                        return Err(PreflightError::Malformed(
                            crate::BiffStructureError::FunctionArity,
                        ));
                    }
                    n
                } else {
                    if min != max {
                        return Ok(None);
                    }
                    min
                };

                let start = s.len().checked_sub(n).ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::FunctionArgumentStack,
                ))?;
                // SAFETY: start is computed by checked subtraction from s.len(), without intervening mutation of s.
                let args = s
                    .split_off(start)
                    .into_iter()
                    .map(|item| match item {
                        Item::Expression(value) => Ok(value),
                        Item::AddIn(_) => Err(PreflightError::Representation(
                            crate::BiffRepresentationError::NamexUsedAsOrdinaryArgument,
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?;
                s.push(Item::Expression(format!("{name}({})", args.join(","))));
            }
            _ => return Ok(None),
        }
        let bytes = s
            .iter()
            .try_fold(0usize, |n, v| {
                n.checked_add(match v {
                    Item::Expression(v) | Item::AddIn(v) => v.len(),
                })
            })
            .ok_or(PreflightError::Representation(
                crate::BiffRepresentationError::FormulaTextOverflow,
            ))?;
        if bytes > budget.max_string_bytes {
            return Err(PreflightError::Resource {
                resource: crate::BiffResource::BiffFormulaBytes,
                requested: bytes as u64,
                limit: budget.max_string_bytes as u64,
            });
        }
    }
    if s.len() != 1 {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::FormulaFinalStack,
        ));
    }
    Ok(Some(pop(&mut s)?))
}

#[cfg(test)]
mod memory_wrapper_tests {
    use super::*;

    #[test]
    fn memory_tokens_annotate_without_skipping_or_changing_the_stack() {
        for prefix in [vec![0x29, 3, 0], vec![0x26, 0, 0, 0, 0, 3, 0]] {
            let mut bytes = prefix.clone();
            bytes.extend([0x1e, 7, 0]);
            assert_eq!(
                formula(&bytes, &[], &[], &[], ResourceBudget::default()).unwrap(),
                Some("7".into())
            );
            for length in 1..prefix.len() {
                assert!(
                    formula(&prefix[..length], &[], &[], &[], ResourceBudget::default()).is_err()
                );
            }
            // The next unknown token is still visited: wrapper lengths are not a skip instruction.
            bytes[prefix.len()] = 0x02;
            assert_eq!(
                formula(&bytes, &[], &[], &[], ResourceBudget::default()).unwrap(),
                None
            );
        }
    }
}
