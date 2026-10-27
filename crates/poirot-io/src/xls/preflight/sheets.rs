//! Per-sheet row inventory, record validation and text charges.
use std::collections::BTreeMap;

use super::biff_plan::BiffPlan;
use super::checks::{check, u16at, u32at};
use super::preflight_error::PreflightError;
use super::record::Record;
use super::resource_budget::ResourceBudget;
use super::substream::{Substream, sheet_starts};

/// Inspect worksheet records and accumulate bounded text-allocation estimates.
/// Every traversal is charged, so BoundSheets sharing one start pay per visit.
pub(super) fn sheets(
    rs: &[Record<'_>],
    positions: &[usize],
    sizes: &[usize],
    budget: ResourceBudget,
    mut plan: BiffPlan,
) -> Result<BiffPlan, PreflightError> {
    plan.sheets = positions.len();
    plan.sheet_starts = sheet_starts(rs, positions)?;
    let mut visits = 0usize;
    let mut cells = 0usize;
    let mut cell_text = 0u64;
    for &start in &plan.sheet_starts {
        let mut rows = BTreeMap::new();
        let mut substream = Substream::new(rs, start);
        while let Some(r) = substream.next() {
            check(
                "BIFF scan work",
                visits.saturating_add(substream.visited()) as u64,
                budget.max_records as u64,
            )?;
            if r.id == 0x0208 {
                if r.data.len() != 16 {
                    return Err(PreflightError::Malformed("ROW length"));
                }
                // ROW flags (MS-XLS 2.4.221): fDyZero, bit 5 of the byte at offset 12, hides the row.
                let hidden = r.data.get(12).is_some_and(|flags| flags & 0x20 != 0);
                rows.insert(
                    u16at(r.data, 0)? as u32,
                    crate::RowVisibility {
                        manually_hidden: hidden,
                        filtered_out: false,
                    },
                );
            }
            if r.id == 0xe5 && r.data.len() < 2 + u16at(r.data, 0)? as usize * 8 {
                return Err(PreflightError::Malformed("merge count exceeds record"));
            }
            if r.id == 0xfd {
                let index = u32at(r.data, 6)? as usize;
                cell_text = cell_text.saturating_add(sizes.get(index).copied().unwrap_or(0) as u64);
            }
            if matches!(r.id, 0x204 | 0xd6 | 0x207) {
                let offset = if r.id == 0x207 { 0 } else { 6 };
                cell_text = cell_text.saturating_add(u16at(r.data, offset)? as u64 * 4);
            }
            check("cell text bytes", cell_text, budget.max_cell_text_bytes)?;
            // Decoding stores sparse cells and does not reserve from Dimensions.
            if r.id == 0x200 && !matches!(r.data.len(), 10 | 14) {
                return Err(PreflightError::Malformed("Dimensions length"));
            }
            if r.id == 0x6 {
                let n = u16at(r.data, 20)? as usize;
                if r.data.len() < 22 + n {
                    return Err(PreflightError::Malformed(
                        "formula token claim exceeds record",
                    ));
                }
            }
            let minimum = match r.id {
                0x203 => 14,
                0x204 | 0xd6 => 8,
                0x205 => 8,
                0x27e => 10,
                0xfd => 10,
                0x6 => 22,
                0xbd => 12,
                _ => 0,
            };
            if minimum > 0 {
                if r.data.len() < minimum {
                    return Err(PreflightError::Malformed("truncated cell record"));
                }
                let col = u16at(r.data, 2)? as u32;
                if r.id == 0xbd {
                    let last = u16at(r.data, r.data.len() - 2)? as u32;
                    if last < col
                        || (last - col + 1) as usize != (r.data.len() - 6) / 6
                        || (r.data.len() - 6) % 6 != 0
                    {
                        return Err(PreflightError::Malformed("MulRK coordinates/length"));
                    }
                }
                // Each cell record decodes one cell; MulRK decodes one per RK pair.
                let decoded = if r.id == 0xbd {
                    // SAFETY: the minimum check above returned unless MulRK data has at least 12 bytes.
                    (r.data.len() - 6) / 6
                } else {
                    1
                };
                cells = cells.saturating_add(decoded);
                check("decoded cells", cells as u64, budget.max_cells as u64)?;
            }
        }
        visits = visits.saturating_add(substream.visited());
        check("BIFF scan work", visits as u64, budget.max_records as u64)?;
        plan.rows.push(rows);
    }
    Ok(plan)
}
