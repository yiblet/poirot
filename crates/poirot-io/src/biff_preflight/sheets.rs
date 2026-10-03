//! Per-sheet row inventory, reservation admission, dense bounds and text charges.
use std::collections::BTreeSet;

use super::{BiffPlan, PreflightError, Record, ResourceBudget, check, u16at, u32at};

/// Inspect worksheet records and accumulate bounded backend allocation estimates.
pub(super) fn sheets(
    rs: &[Record<'_>],
    positions: Vec<usize>,
    sizes: &[usize],
    budget: ResourceBudget,
    mut plan: BiffPlan,
) -> Result<BiffPlan, PreflightError> {
    plan.sheets = positions.len();
    let mut visits = 0usize;
    let mut cell_text = 0u64;
    for pos in positions {
        let mut rows = BTreeSet::new();
        let mut minrow = u32::MAX;
        let mut mincol = u32::MAX;
        let mut maxrow = 0;
        let mut maxcol = 0;
        let mut cells = false;
        let mut formula_position = (0u32, 0u32);
        for r in rs
            .iter()
            .skip_while(|r| r.at < pos)
            .take_while(|r| r.id != 0xa)
        {
            visits += 1;
            check("BIFF scan work", visits as u64, budget.max_records as u64)?;
            if r.id == 0x0208 {
                if r.data.len() != 16 {
                    return Err(PreflightError::Malformed("ROW length"));
                }
                rows.insert(u16at(r.data, 0)? as u32);
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
            if r.id == 0x200 {
                let (rf, rl, mut cf, cl) = match r.data.len() {
                    10 => (
                        u16at(r.data, 0)? as u32,
                        u16at(r.data, 2)? as u32,
                        u16at(r.data, 4)? as u32,
                        u16at(r.data, 6)? as u32,
                    ),
                    14 => (
                        u32at(r.data, 0)?,
                        u32at(r.data, 4)?,
                        u16at(r.data, 8)? as u32,
                        u16at(r.data, 10)? as u32,
                    ),
                    _ => return Err(PreflightError::Malformed("Dimensions length")),
                };
                // Match backend's permissive invalid first-column fallback.
                if cf > 255 || cl < cf {
                    cf = 0;
                }
                let area = if rl > 0 && cl > 0 {
                    let rows = rl.checked_sub(rf).filter(|n| *n > 0).ok_or(
                        PreflightError::Representation("Dimensions row subtraction underflow"),
                    )?;
                    let cols = cl - cf;
                    (rows as u64) * (cols as u64)
                } else {
                    1
                };
                check("Dimensions reservation cells", area, budget.max_dense_cells)?;
            }
            if r.id == 0x207 {
                let (row, col) = formula_position;
                cells = true;
                minrow = minrow.min(row);
                maxrow = maxrow.max(row);
                mincol = mincol.min(col);
                maxcol = maxcol.max(col);
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
                let row = u16at(r.data, 0)? as u32;
                let col = u16at(r.data, 2)? as u32;
                if r.id == 0x6 {
                    formula_position = (row, col);
                }
                let last = if r.id == 0xbd {
                    let last = u16at(r.data, r.data.len() - 2)? as u32;
                    if last < col
                        || (last - col + 1) as usize != (r.data.len() - 6) / 6
                        || (r.data.len() - 6) % 6 != 0
                    {
                        return Err(PreflightError::Malformed("MulRK coordinates/length"));
                    }
                    last
                } else {
                    col
                };
                cells = true;
                minrow = minrow.min(row);
                maxrow = maxrow.max(row);
                mincol = mincol.min(col);
                maxcol = maxcol.max(last);
            }
        }
        if cells {
            let area = (maxrow - minrow + 1) as u64 * (maxcol - mincol + 1) as u64;
            check("sheet dense cells", area, budget.max_dense_cells)?;
            // Both value and formula dense ranges can cover the same rectangle.
            plan.dense_cells = plan
                .dense_cells
                .checked_add(area * 2)
                .ok_or(PreflightError::Representation("total dense cells overflow"))?;
            check(
                "total dense cells",
                plan.dense_cells,
                budget.max_total_dense_cells,
            )?;
        }
        plan.row_indices.push(rows);
    }
    Ok(plan)
}
