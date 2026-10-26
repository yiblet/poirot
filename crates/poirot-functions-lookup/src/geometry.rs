use crate::Reference;
use crate::{Context, Error, Failure};
/// Translate and normalize an OFFSET axis within an exclusive worksheet limit.
/// Negative lengths retain the displaced cell as the last endpoint; zero is invalid.
/// Positive dimensions and grid bounds follow Microsoft's OFFSET documentation:
/// <https://support.microsoft.com/en-us/excel/functions/offset-function>.
/// Negative dimensions remain a compatibility policy, unverified in native Excel.
pub fn offset_extent(
    start: i64,
    displacement: i64,
    length: i64,
    limit: u32,
) -> Result<(u32, u32), Error> {
    if length == 0 {
        return Err(Error::Ref);
    }
    let displaced = start.checked_add(displacement).ok_or(Error::Ref)?;
    let (first, last) = if length < 0 {
        (
            displaced
                .checked_add(length)
                .and_then(|n| n.checked_add(1))
                .ok_or(Error::Ref)?,
            displaced,
        )
    } else {
        (
            displaced,
            displaced
                .checked_add(length)
                .and_then(|n| n.checked_sub(1))
                .ok_or(Error::Ref)?,
        )
    };
    if first < 0 || last >= i64::from(limit) {
        return Err(Error::Ref);
    }
    Ok((
        u32::try_from(first).map_err(|_| Error::Ref)?,
        u32::try_from(last).map_err(|_| Error::Ref)?,
    ))
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
/// Resolve legacy scalar intersection for workbook-independent lookup consumers.
pub(super) fn scalar_selection(r: &Reference, context: Context) -> Result<CellSelection, Failure> {
    let (row, col) = if r.height() == 1 && r.width() == 1 {
        (r.first_row(), r.first_col())
    } else if r.width() == 1 && (r.first_row()..=r.last_row()).contains(&context.row) {
        (context.row, r.first_col())
    } else if r.height() == 1 && (r.first_col()..=r.last_col()).contains(&context.col) {
        (r.first_row(), context.col)
    } else if (r.first_row()..=r.last_row()).contains(&context.row)
        && (r.first_col()..=r.last_col()).contains(&context.col)
    {
        (context.row, context.col)
    } else {
        return Err(Error::Value.into());
    };

    Ok(CellSelection {
        sheet: r.first_sheet(),
        row,
        col,
    })
}
/// Select one-based offsets for an unambiguous scalar, row, or column reference.
/// A multi-row, multi-column rectangle has no unique implicit intersection.
pub fn intersection_offsets(
    rows: (i64, i64),
    columns: (i64, i64),
    at: (i64, i64),
) -> Option<(usize, usize)> {
    if rows.0 > rows.1 || columns.0 > columns.1 {
        return None;
    }
    let (row, column) = if rows.0 == rows.1 && columns.0 == columns.1 {
        (rows.0, columns.0)
    } else if columns.0 == columns.1 && (rows.0..=rows.1).contains(&at.0) {
        (at.0, columns.0)
    } else if rows.0 == rows.1 && (columns.0..=columns.1).contains(&at.1) {
        (rows.0, at.1)
    } else {
        return None;
    };
    Some((
        usize::try_from(row.checked_sub(rows.0)?.checked_add(1)?).ok()?,
        usize::try_from(column.checked_sub(columns.0)?.checked_add(1)?).ok()?,
    ))
}
