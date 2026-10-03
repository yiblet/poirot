use crate::domain::CheckedReference;
use crate::{Context, Error, Failure};
/// One signed OFFSET axis request, validated before constructing a reference.
pub(super) struct OffsetAxis {
    /// Original signed coordinate on this axis.
    pub(super) start: i32,
    /// Requested displacement from the original coordinate.
    pub(super) delta: i32,
    /// Requested output extent on this axis.
    pub(super) length: i32,
    /// Exclusive worksheet coordinate limit.
    pub(super) limit: i32,
}
/// Source coordinates before translation into the unsigned workbook reference port.
/// Negative lower endpoints are supported by POI and read as blank outside the sheet.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct OffsetExtent {
    /// First translated endpoint, inclusive.
    pub(super) first: i32,
    /// Last translated endpoint, inclusive.
    pub(super) last: i32,
}
impl OffsetAxis {
    /// Validate displacement and extent with pinned boundary behavior.
    pub(super) fn plan(self) -> Result<OffsetExtent, Failure> {
        let Self {
            start,
            delta,
            length: len,
            limit,
        } = self;
        if len == 0 {
            return Err(Error::Ref.into());
        }
        // Every addition/negation is Java int arithmetic; getLastIndex narrows to short.
        let normalized_first = if len < 0 {
            start.wrapping_add(delta).wrapping_add(len).wrapping_add(1)
        } else {
            start.wrapping_add(delta)
        };
        let normalized_length = if len < 0 { len.wrapping_neg() } else { len };
        let normalized_last = normalized_first
            .wrapping_add(normalized_length)
            .wrapping_sub(1) as i16 as i32;
        if normalized_first < 0 || normalized_last >= limit {
            return Err(Error::Ref.into());
        }
        // AreaI.OffsetArea receives the ORIGINAL short-narrowed relative endpoints.
        let relative_first = delta as i16 as i32;
        let relative_last = delta.wrapping_add(len).wrapping_sub(1) as i16 as i32;
        let first = start.wrapping_add(relative_first.min(relative_last));
        let last = start.wrapping_add(relative_first.max(relative_last));
        Ok(OffsetExtent { first, last })
    }
}
/// Resolved single-cell coordinates passed from reference geometry to host reads.
pub(super) struct CellSelection {
    /// Zero-based selected sheet index.
    pub(super) sheet: u32,
    /// Zero-based selected row coordinate.
    pub(super) row: u32,
    /// Zero-based selected column coordinate.
    pub(super) col: u32,
}
/// Ordinary scalar intersection has stricter vector selection than SINGLE.
pub(super) fn scalar_selection(
    checked: &CheckedReference<'_>,
    context: Context,
) -> Result<CellSelection, Failure> {
    let r = checked.reference();
    let (row, col) = if checked.height() == 1 && checked.width() == 1 {
        (r.first_row, r.first_col)
    } else if checked.width() == 1 && (r.first_row..=r.last_row).contains(&context.row) {
        (context.row, r.first_col)
    } else if checked.height() == 1 && (r.first_col..=r.last_col).contains(&context.col) {
        (r.first_row, context.col)
    } else if (r.first_row..=r.last_row).contains(&context.row)
        && (r.first_col..=r.last_col).contains(&context.col)
    {
        (context.row, context.col)
    } else {
        return Err(Error::Value.into());
    };

    Ok(CellSelection {
        sheet: r.first_sheet,
        row,
        col,
    })
}
/// SINGLE also admits the projection of either contained coordinate.
pub(super) fn single_selection(checked: &CheckedReference<'_>, c: Context) -> Option<(u32, u32)> {
    let r = checked.reference();
    let contains_row = (r.first_row..=r.last_row).contains(&c.row);
    let contains_col = (r.first_col..=r.last_col).contains(&c.col);
    if contains_row && contains_col {
        Some((c.row, c.col))
    } else if contains_row {
        Some((c.row, r.first_col))
    } else if contains_col {
        Some((r.first_row, c.col))
    } else {
        None
    }
}

#[cfg(test)]
mod offset_tests {
    use super::*;
    fn plan(start: i32, delta: i32, length: i32) -> Result<OffsetExtent, Failure> {
        OffsetAxis {
            start,
            delta,
            length,
            limit: 1_048_576,
        }
        .plan()
    }
    #[test]
    fn exact_short_narrowing_retains_original_endpoints() {
        for (start, delta, length, first, last) in [
            (2, 0, -2, -1, 2),
            (1, 0, -2, -2, 1),
            (4, 1, -2, 2, 5),
            (0, 32768, 1, -32768, -32768),
            (0, 65535, 1, -1, -1),
            (0, 65536, 1, 0, 0),
            (0, 32767, 2, -32768, 32767),
            (32768, -32768, 1, 0, 0),
            (32766, 0, -32767, -2, 32766),
            (32767, 0, -32768, 32767, 65534),
            (0, i32::MAX, 1, -1, -1),
            (0, i32::MAX, i32::MIN, -2, -1),
            (i32::MAX, 0, 2, i32::MAX, i32::MIN),
        ] {
            assert_eq!(
                plan(start, delta, length),
                Ok(OffsetExtent { first, last }),
                "{start} {delta} {length}"
            );
        }
    }
    #[test]
    fn raw_normalized_first_and_short_normalized_last_are_distinct() {
        for (start, delta, length) in [(0, 0, 0), (0, 0, -2), (0, 0, i32::MIN), (1, i32::MAX, 1)] {
            assert_eq!(plan(start, delta, length), Err(Error::Ref.into()));
        }
        // Column bounds compare SHORT last, rather than raw positive first.
        assert_eq!(
            OffsetAxis {
                start: 0,
                delta: 16384,
                length: 1,
                limit: 16384
            }
            .plan(),
            Err(Error::Ref.into())
        );
        assert_eq!(
            OffsetAxis {
                start: 16383,
                delta: 16385,
                length: 1,
                limit: 16384
            }
            .plan(),
            Ok(OffsetExtent {
                first: 32768,
                last: 32768
            })
        );
    }
}
