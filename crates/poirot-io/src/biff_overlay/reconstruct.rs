//! Ordered global declarations, name formulas and sheet reconstruction.
use super::backend::BackendPatch;
use super::formula::{addins::AddIns, formula_with_addins};
use super::text::{cached_text, take, text, units, word};
use super::{Formula, PreflightError, ResourceBudget, Result};
use crate::biff_preflight::Record;
use poirot_model::{CellAddress, ExcelText, NamedRange};

fn charge(total: &mut u64, bytes: usize, limit: u64, resource: &'static str) -> Result<()> {
    *total = total
        .checked_add(bytes as u64)
        .ok_or(PreflightError::Representation(
            "overlay text total overflow",
        ))?;
    if *total > limit {
        return Err(PreflightError::Resource {
            resource,
            requested: *total,
            limit,
        });
    }
    Ok(())
}

/// BIFF source recovery plan applied around backend decoding.
pub(super) struct Reconstruction {
    /// Source worksheet spellings in index order.
    pub(super) sheets: Vec<String>,
    /// Recovered workbook and sheet-scoped defined names.
    pub(super) names: Vec<NamedRange>,
    /// Recovered formula source assigned to sheet coordinates.
    pub(super) formulas: Vec<Formula>,
    /// Exact UTF-16 formula text caches to restore after decoding.
    pub(super) caches: Vec<(usize, CellAddress, ExcelText)>,
    /// Fidelity omissions retained for the import report.
    pub(super) warnings: Vec<String>,
    /// Backend-only record replacements required for safe decoding.
    pub(super) patches: Vec<BackendPatch>,
}

/// Inspect checked BIFF records and plan source recovery without mutating the workbook.
pub(super) fn reconstruct(rs: &[Record<'_>], budget: ResourceBudget) -> Result<Reconstruction> {
    let mut sheets = Vec::new();
    let mut positions = Vec::new();
    let mut names = Vec::new();
    let mut name_tokens = Vec::new();
    let mut xtis = Vec::new();
    let mut addins = AddIns::default();
    let mut patches = Vec::new();
    let mut warnings = Vec::new();
    let mut formula_bytes = 0u64;
    let mut cache_bytes = 0u64;
    for r in rs.iter().take_while(|r| r.id != 0xa) {
        addins.observe(r.id, r.data, budget)?;
        match r.id {
            0x42 => {
                let mut codepage = r.data;
                if word(&mut codepage)? != 1200 {
                    warnings.push("BIFF8 CODEPAGE differs from UTF16; raw sheet/name/formula/cell/cache text is overlaid".into());
                }
            }
            0x85 => {
                let mut b = r.data;
                let v = take(&mut b, 4)?;
                positions.push(u32::from_le_bytes(
                    v.try_into()
                        .map_err(|_| PreflightError::Malformed("sheet position"))?,
                ) as usize);
                take(&mut b, 2)?;
                let n = *take(&mut b, 1)?
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?
                    as usize;
                sheets.push(text(&mut b, n)?);
            }
            0x17 => {
                let mut b = r.data;
                let count = word(&mut b)? as usize;
                let total = xtis
                    .len()
                    .checked_add(count)
                    .ok_or(PreflightError::Representation("XTI count overflow"))?;
                if total > budget.max_metadata_entries {
                    return Err(PreflightError::Resource {
                        resource: "XTI declarations",
                        requested: total as u64,
                        limit: budget.max_metadata_entries as u64,
                    });
                }
                if b.len() != count * 6 {
                    return Err(PreflightError::Malformed("EXTERNSHEET record length"));
                }
                for _ in 0..count {
                    xtis.push((word(&mut b)?, word(&mut b)?, word(&mut b)?));
                }
            }
            0x18 => {
                let mut b = r.data;
                let flags = word(&mut b)?;
                take(&mut b, 1)?;
                let n = *take(&mut b, 1)?
                    .first()
                    .ok_or(PreflightError::Malformed("missing byte"))?
                    as usize;
                let tokens = word(&mut b)? as usize;
                take(&mut b, 2)?;
                let scope = word(&mut b)?;
                take(&mut b, 4)?;
                let decoded = text(&mut b, n)?;
                let name = if flags & 0x20 != 0 {
                    match decoded.chars().next().map(u32::from) {
                        Some(6) => "_xlnm.Print_Area".into(),
                        Some(7) => "_xlnm.Print_Titles".into(),
                        Some(13) => "_xlnm._FilterDatabase".into(),
                        _ => {
                            warnings.push("Unsupported built-in BIFF name identity retained as raw code point".into());
                            decoded
                        }
                    }
                } else {
                    decoded
                };
                charge(
                    &mut formula_bytes,
                    name.len(),
                    budget.max_string_bytes as u64,
                    "overlay formula/name bytes",
                )?;
                names.push(NamedRange {
                    name,
                    formula: String::new(),
                    sheet_scope: scope.checked_sub(1).map(usize::from),
                });
                name_tokens.push(take(&mut b, tokens)?);
                patches.push(BackendPatch::SuppressName { at: r.at });
            }
            _ => {}
        }
    }
    for (i, tokens) in name_tokens.iter().enumerate() {
        if let Some(f) = formula_with_addins(tokens, &sheets, &names, &xtis, &addins, budget)? {
            charge(
                &mut formula_bytes,
                f.len(),
                budget.max_string_bytes as u64,
                "overlay formula/name bytes",
            )?;
            // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
            names[i].formula = f;
        } else {
            warnings.push(format!(
                "Unsupported BIFF defined-name tokens for {}",
                // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
                names[i].name
            ));
        }
    }
    let mut shared = Vec::new();
    for (at, _) in rs.iter().enumerate().filter(|(_, r)| r.id == 0xfc) {
        // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
        let end = rs[at + 1..]
            .iter()
            .position(|r| r.id != 0x3c)
            .map_or(rs.len(), |n| at + 1 + n);
        // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
        shared = super::sst::sst(&rs[at..end], budget)?;
    }
    let mut formulas = Vec::new();
    let mut caches = Vec::new();
    for (sheet, pos) in positions.iter().enumerate() {
        let mut pending = None;
        let start = rs
            .binary_search_by_key(pos, |r| r.at)
            .map_err(|_| PreflightError::Malformed("sheet boundary"))?;
        let mut i = start;
        // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
        while i < rs.len() && rs[i].id != 0xa {
            // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
            let r = rs[i];
            if matches!(r.id, 0xfd | 0x204 | 0xd6) {
                let mut b = r.data;
                let row = word(&mut b)?;
                let col = word(&mut b)?;
                let address = CellAddress::new(row as u32, col as u32)
                    .map_err(|_| PreflightError::Representation("text coordinate"))?;
                take(&mut b, 2)?;
                let text = if r.id == 0xfd {
                    let raw = take(&mut b, 4)?;
                    // SAFETY: names and name_tokens are pushed together locally; record cursors come from enumeration or checked binary search and are guarded by rs.len(). The four-byte conversion follows take(..., 4).
                    let index = u32::from_le_bytes(raw.try_into().expect("four bytes")) as usize;
                    shared
                        .get(index)
                        .cloned()
                        .ok_or(PreflightError::Malformed("shared string index"))?
                } else {
                    let n = word(&mut b)? as usize;
                    if n > budget.max_string_chars {
                        return Err(PreflightError::Resource {
                            resource: "LABEL string characters",
                            requested: n as u64,
                            limit: budget.max_string_chars as u64,
                        });
                    }
                    units(&mut b, n)?
                };
                charge(
                    &mut cache_bytes,
                    text.units().len() * 2,
                    budget.max_cell_text_bytes,
                    "overlay cached text bytes",
                )?;
                caches.push((sheet, address, text));
            }
            if r.id == 6 {
                let mut b = r.data;
                let row = word(&mut b)?;
                let col = word(&mut b)?;
                let address = CellAddress::new(row as u32, col as u32)
                    .map_err(|_| PreflightError::Representation("formula coordinate"))?;
                take(&mut b, 16)?;
                let n = word(&mut b)? as usize;
                let tokens = take(&mut b, n)?;
                pending = if r.data.get(12..14) == Some(&[255, 255]) && r.data.get(6) == Some(&0) {
                    Some(address)
                } else {
                    None
                };
                if let Some(text) =
                    formula_with_addins(tokens, &sheets, &names, &xtis, &addins, budget)?
                {
                    charge(
                        &mut formula_bytes,
                        text.len(),
                        budget.max_string_bytes as u64,
                        "overlay formula/name bytes",
                    )?;
                    formulas.push(Formula {
                        sheet,
                        address,
                        text,
                    });
                    if n >= 3 {
                        patches.push(BackendPatch::FormulaPlaceholder { at: r.at });
                    }
                } else {
                    warnings.push(format!(
                        "Unsupported BIFF formula tokens on sheet {sheet} at {address}"
                    ));
                }
            }
            if r.id == 0x207 {
                let address = pending
                    .take()
                    .ok_or(PreflightError::Malformed("cached STRING without FORMULA"))?;
                let cached = cached_text(rs, i, budget)?;
                i = cached.last_record;
                let text = cached.text;
                charge(
                    &mut cache_bytes,
                    text.units().len() * 2,
                    budget.max_cell_text_bytes,
                    "overlay cached text bytes",
                )?;
                caches.push((sheet, address, text));
            }
            i += 1;
        }
    }
    Ok(Reconstruction {
        sheets,
        names,
        formulas,
        caches,
        warnings,
        patches,
    })
}
