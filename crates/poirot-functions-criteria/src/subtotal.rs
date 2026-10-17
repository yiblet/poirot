//! Workbook adapters supply real visibility and nested-formula metadata.
use crate::{Cell, Error};
/// A value plus adapter-supplied visibility, nested-SUBTOTAL, and reference-origin metadata.
/// The SUBTOTAL code decides which eligible values contribute; metadata cannot be
/// inferred from the numeric value alone.
#[derive(Clone, Copy, Debug)]
pub struct SubtotalCell<'a> {
    /// Projected operand before SUBTOTAL coercion.
    pub value: &'a Cell,
    /// Whether the source row is hidden under the caller's visibility rules.
    pub hidden: bool,
    /// Whether this value comes from a nested subtotal to exclude.
    pub nested_subtotal: bool,
    /// Whether reference-specific coercion applies to this operand.
    pub referenced: bool,
}
/// Aggregate admitted cells while respecting hidden rows and nested subtotals.
pub fn subtotal(code: i32, cells: &[SubtotalCell<'_>]) -> Result<f64, Error> {
    let hidden = code > 100;
    let kind = if hidden { code - 100 } else { code };
    if !(1..=11).contains(&kind) {
        return Err(Error::Value);
    }
    let mut nums = Vec::new();
    let mut nonblank = 0usize;
    for c in cells {
        if c.nested_subtotal || (hidden && c.hidden) {
            continue;
        }
        if !matches!(c.value, Cell::Blank) {
            nonblank += 1;
        }
        if let Some(number) = coerce_subtotal_member(c, kind)? {
            nums.push(number);
        }
    }
    let n = nums.len();
    let sum: f64 = nums.iter().sum();
    let result = match kind {
        1 => {
            if n == 0 {
                return Err(Error::DivZero);
            }
            sum / n as f64
        }
        2 => n as f64,
        3 => nonblank as f64,
        4 => nums.iter().copied().reduce(f64::max).unwrap_or(0.0),
        5 => nums.iter().copied().reduce(f64::min).unwrap_or(0.0),
        6 => {
            if n == 0 {
                0.0
            } else {
                nums.iter().product()
            }
        }
        9 => sum,
        7 | 8 | 10 | 11 => {
            let sample = kind == 7 || kind == 10;
            if n == 0 || (sample && n < 2) {
                return Err(Error::DivZero);
            }
            let mean = sum / n as f64;
            let variance = nums.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                / (n - usize::from(sample)) as f64;
            if kind == 7 || kind == 8 {
                variance.sqrt()
            } else {
                variance
            }
        }
        // SAFETY: kind was checked to be in 1..=11, and all eleven values are handled above.
        _ => unreachable!(),
    };
    if result.is_finite() {
        Ok(result)
    } else {
        Err(Error::Num)
    }
}

/// Origin-sensitive admission and conversion for one eligible SUBTOTAL member.
fn coerce_subtotal_member(cell: &SubtotalCell<'_>, kind: i32) -> Result<Option<f64>, Error> {
    Ok(match cell.value {
        Cell::Number(n) => Some(*n),
        Cell::Bool(b) if !cell.referenced && kind != 2 && kind != 3 => {
            Some(if *b { 1.0 } else { 0.0 })
        }
        Cell::Text(s) if !cell.referenced && kind != 2 && kind != 3 => {
            Some(crate::coerce_criteria_number_text(s).ok_or(Error::Value)?)
        }
        Cell::Error(e) if kind != 2 && kind != 3 => return Err(*e),
        _ => None,
    })
}
