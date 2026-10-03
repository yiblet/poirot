//! BIFF token reconstruction and reference rendering.
use super::text::{take, text, word};
use super::{PreflightError, ResourceBudget, Result};
use poirot_model::NamedRange;

/// BIFF external-name bookkeeping used to reconstruct add-in function spellings.
pub(super) mod addins;
mod metadata;
use metadata::function_metadata;

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
/// Formula reconstruction stack member retaining text and grouping state.
enum Item {
    Expression(String),
    AddIn(String),
}
fn pop(s: &mut Vec<Item>) -> Result<String> {
    match s.pop() {
        Some(Item::Expression(value)) => Ok(value),
        Some(Item::AddIn(_)) => Err(PreflightError::Representation(
            "NameX used outside external function call",
        )),
        None => Err(PreflightError::Malformed("formula stack underflow")),
    }
}
/// Reconstruct supported RPN tokens into formula source without add-in bindings.
#[cfg(test)]
pub(super) fn formula(
    b: &[u8],
    sheets: &[String],
    names: &[NamedRange],
    xtis: &[(u16, u16, u16)],
    budget: ResourceBudget,
) -> Result<Option<String>> {
    formula_with_addins(b, sheets, names, xtis, &addins::AddIns::default(), budget)
}
/// Reconstruct RPN source using checked add-in function identities.
pub(super) fn formula_with_addins(
    mut b: &[u8],
    sheets: &[String],
    names: &[NamedRange],
    xtis: &[(u16, u16, u16)],
    addins: &addins::AddIns,
    budget: ResourceBudget,
) -> Result<Option<String>> {
    if b.is_empty() {
        return Ok(None);
    }
    let mut s = Vec::new();
    let mut token_count = 0usize;
    while !b.is_empty() {
        token_count = token_count
            .checked_add(1)
            .ok_or(PreflightError::Representation(
                "formula token count overflow",
            ))?;
        if token_count > budget.max_metadata_entries {
            return Err(PreflightError::Resource {
                resource: "overlay formula tokens",
                requested: token_count as u64,
                limit: budget.max_metadata_entries as u64,
            });
        }
        let raw = *take(&mut b, 1)?
            .first()
            .ok_or(PreflightError::Malformed("missing byte"))?;
        let id = if raw >= 0x20 {
            0x20 | (raw & 0x1f)
        } else {
            raw
        };
        match id {
            // Pinned MemArea/MemFunc payloads annotate, but do not consume,
            // the following reference expression. Decode it normally and retain
            // all per-token and stack-text budget checks.
            0x26 | 0x29 => {
                if id == 0x26 {
                    take(&mut b, 4)?;
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
            0x17 => {
                let n = *take(&mut b, 1)?
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?
                    as usize;
                let t = text(&mut b, n)?;
                s.push(Item::Expression(format!("\"{}\"", t.replace('"', "\"\""))));
            }
            0x19 => {
                let flags = *take(&mut b, 1)?
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?;
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
                if *take(&mut b, 1)?
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?
                    == 0
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
                    f64::from_le_bytes(
                        v.try_into()
                            .map_err(|_| PreflightError::Malformed("number token"))?,
                    )
                    .to_string(),
                ));
            }
            0x23 => {
                let v = take(&mut b, 4)?;
                let i = u32::from_le_bytes(
                    v.try_into()
                        .map_err(|_| PreflightError::Malformed("name token"))?,
                ) as usize;
                let Some(name) = i.checked_sub(1).and_then(|i| names.get(i)) else {
                    return Ok(None);
                };
                s.push(Item::Expression(name.name.clone()));
            }
            0x24 | 0x25 | 0x3a | 0x3b => {
                let prefix = if id >= 0x3a {
                    let i = word(&mut b)? as usize;
                    let Some((sup, first, last)) = xtis.get(i) else {
                        return Ok(None);
                    };
                    if *sup != 0 || first != last {
                        return Ok(None);
                    }
                    let Some(sheet) = sheets.get(*first as usize) else {
                        return Ok(None);
                    };
                    format!("'{}'!", sheet.replace('\'', "''"))
                } else {
                    String::new()
                };
                let area = id == 0x25 || id == 0x3b;
                let r1 = word(&mut b)?;
                let r2 = if area { word(&mut b)? } else { r1 };
                let c1 = word(&mut b)?;
                let c2 = if area { word(&mut b)? } else { c1 };
                let a = reference(r1, c1);
                s.push(Item::Expression(if area {
                    format!("{prefix}{a}:{}", reference(r2, c2))
                } else {
                    format!("{prefix}{a}")
                }));
            }
            0x39 => {
                let xti = word(&mut b)? as usize;
                let slot = take(&mut b, 4)?;
                let slot = u32::from_le_bytes(
                    slot.try_into()
                        .map_err(|_| PreflightError::Malformed("NameX slot"))?,
                );
                let xti = *xtis
                    .get(xti)
                    .ok_or(PreflightError::Malformed("NameX XTI"))?;
                let name = addins.resolve(xti, slot)?;
                if name.len() > budget.max_string_bytes {
                    return Err(PreflightError::Resource {
                        resource: "overlay formula bytes",
                        requested: name.len() as u64,
                        limit: budget.max_string_bytes as u64,
                    });
                }
                s.push(Item::AddIn(name.to_owned()));
            }
            0x21 | 0x22 => {
                let count = if id == 0x22 {
                    Some(
                        *take(&mut b, 1)?
                            .first()
                            .ok_or(PreflightError::Malformed("missing byte"))?
                            as usize,
                    )
                } else {
                    None
                };
                let index = word(&mut b)?;
                if index == 255 {
                    let count = count.ok_or(PreflightError::Malformed(
                        "external function must use FuncVar",
                    ))?;
                    let start = s
                        .len()
                        .checked_sub(count)
                        .ok_or(PreflightError::Malformed("external function stack"))?;
                    let Some(Item::AddIn(name)) = s.get(start) else {
                        return Err(PreflightError::Representation(
                            "UDF function identity unsupported",
                        ));
                    };
                    let (min, max) = addins::arity(name).ok_or(PreflightError::Representation(
                        "unsupported add-in function identity",
                    ))?;
                    if count == 0 || count - 1 < min || count - 1 > max {
                        return Err(PreflightError::Malformed("external function arity"));
                    }
                    // SAFETY: The opcode match bounds id - 3 within the fifteen literal operators; the stack suffix start is checked_sub(count) from the current stack length.
                    let bytes = s[start..]
                        .iter()
                        .try_fold(
                            count.checked_add(1).ok_or(PreflightError::Representation(
                                "external formula size overflow",
                            ))?,
                            |total, item| {
                                total.checked_add(match item {
                                    Item::Expression(text) | Item::AddIn(text) => text.len(),
                                })
                            },
                        )
                        .ok_or(PreflightError::Representation(
                            "external formula size overflow",
                        ))?;
                    // Includes name, arguments, commas and parentheses before split/join/format allocations.
                    if bytes > budget.max_string_bytes {
                        return Err(PreflightError::Resource {
                            resource: "overlay formula bytes",
                            requested: bytes as u64,
                            limit: budget.max_string_bytes as u64,
                        });
                    }
                    // SAFETY: start is computed by checked subtraction from s.len(), without intervening mutation of s.
                    let mut items = s.split_off(start).into_iter();
                    let Some(Item::AddIn(name)) = items.next() else {
                        return Err(PreflightError::Malformed("external function identity"));
                    };
                    let args = items
                        .map(|item| match item {
                            Item::Expression(value) => Ok(value),
                            Item::AddIn(_) => Err(PreflightError::Malformed(
                                "nested external function identity",
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
                        .ok_or(PreflightError::Representation("formula text overflow"))?;
                    if retained > budget.max_string_bytes {
                        return Err(PreflightError::Resource {
                            resource: "overlay formula bytes",
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
                        return Err(PreflightError::Malformed("function arity"));
                    }
                    n
                } else {
                    if min != max {
                        return Ok(None);
                    }
                    min
                };

                let start = s
                    .len()
                    .checked_sub(n)
                    .ok_or(PreflightError::Malformed("function argument stack"))?;
                // SAFETY: start is computed by checked subtraction from s.len(), without intervening mutation of s.
                let args = s
                    .split_off(start)
                    .into_iter()
                    .map(|item| match item {
                        Item::Expression(value) => Ok(value),
                        Item::AddIn(_) => Err(PreflightError::Representation(
                            "NameX used as ordinary argument",
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
            .ok_or(PreflightError::Representation("formula text overflow"))?;
        if bytes > budget.max_string_bytes {
            return Err(PreflightError::Resource {
                resource: "overlay formula bytes",
                requested: bytes as u64,
                limit: budget.max_string_bytes as u64,
            });
        }
    }
    if s.len() != 1 {
        return Err(PreflightError::Malformed("formula final stack"));
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
