use super::calendar::{Date, epoch, leap, month_days};
use crate::{CellError, CellError::Num};

/// Compute a 360-day difference under the selected US or European convention.
pub(super) fn days360(a: Date, b: Date, european: bool) -> i64 {
    let mut ad = a.d.min(30);
    let mut bd = b.d.min(30);
    let mut by = b.y;
    let mut bm = b.m;
    if !european {
        if a.d == month_days(a.y, a.m) {
            ad = 30;
        }
        if b.d == 31 && ad < 30 {
            bm += 1;
            if bm == 13 {
                bm = 1;
                by += 1;
            }
            bd = 1;
        }
    }
    (by - a.y) * 360 + (bm - a.m) * 30 + bd - ad
}
/// Compute YEARFRAC under the selected day-count basis and source calendar rules.
pub(super) fn yearfrac(
    start: f64,
    end: f64,
    a: Date,
    b: Date,
    basis: i64,
) -> Result<f64, CellError> {
    if !(0..=4).contains(&basis) {
        return Err(Num);
    }
    let (s, e, a, b) = if start <= end {
        (start.floor(), end.floor(), a, b)
    } else {
        (end.floor(), start.floor(), b, a)
    };
    if basis == 2 || basis == 3 {
        return Ok((e - s) / if basis == 2 { 360. } else { 365. });
    }
    if basis == 4 {
        return Ok(days360(a, b, true) as f64 / 360.);
    }
    if basis == 0 {
        let mut ad = a.d;
        let mut bd = b.d;
        if ad == 31 {
            ad = 30;
            if bd == 31 {
                bd = 30;
            }
        } else if ad == 30 && bd == 31 {
            bd = 30;
        } else if a.m == 2 && a.d == month_days(a.y, 2) {
            ad = 30;
            if b.m == 2 && b.d == month_days(b.y, 2) {
                bd = 30;
            }
        }
        return Ok(((b.y - a.y) * 360 + (b.m - a.m) * 30 + bd - ad) as f64 / 360.);
    }
    let greater = b.y > a.y + 1 || (b.y == a.y + 1 && (b.m, b.d) > (a.m, a.d));
    let denominator = if greater {
        let before = |y: i64| y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400);
        let years = b.y - a.y + 1;
        365. + (before(b.y) - before(a.y - 1)) as f64 / years as f64
    } else if (leap(a.y) && (a.y == b.y || a.m <= 2))
        || (leap(b.y) && (b.m > 2 || (b.m == 2 && b.d == 29)))
    {
        366.
    } else {
        365.
    };
    Ok((epoch(b) - epoch(a)) as f64 / denominator)
}
