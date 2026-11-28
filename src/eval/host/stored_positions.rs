use crate::eval::values::Area;
use crate::model::CellAddress;

/// Admitted physical coordinates with offsets measured in the original logical reference.
#[derive(Clone)]
pub(in crate::eval) struct StoredPositions<T = CellAddress> {
    addresses: Vec<T>,
    sheet: usize,
    row_origin: u32,
    column_origin: u32,
}

impl<T> Default for StoredPositions<T> {
    /// Represent a reference with no physical intersection.
    fn default() -> Self {
        Self {
            addresses: Vec::new(),
            sheet: 0,
            row_origin: 0,
            column_origin: 0,
        }
    }
}

impl<T> StoredPositions<T> {
    /// Retain an already collected rectangle without materializing a second coordinate vector.
    pub(super) fn new(addresses: Vec<T>, area: Area, row: i64, column: i64) -> Option<Self> {
        if !addresses.is_empty() {
            // Containment in the collected rectangle bounds every offset. Store the
            // origin modulo 2^32 so a signed off-grid origin needs no extra per-cell work.
            for (coordinate, origin) in [
                (area.r0, row),
                (area.r1, row),
                (area.c0, column),
                (area.c1, column),
            ] {
                let offset = u32::try_from(i64::from(coordinate).checked_sub(origin)?).ok()?;
                usize::try_from(offset).ok()?;
            }
        }
        Some(Self {
            addresses,
            sheet: area.sheet,
            row_origin: row as u32,
            column_origin: column as u32,
        })
    }

    /// Count the collected positions before charging their consumption.
    pub(in crate::eval) fn len(&self) -> usize {
        self.addresses.len()
    }

    /// Yield logical offsets together with the original admitted member.
    pub(in crate::eval) fn members(
        self,
        coordinate: impl Fn(&T) -> CellAddress,
    ) -> impl ExactSizeIterator<Item = (usize, usize, T)> {
        let Self {
            addresses,
            row_origin,
            column_origin,
            ..
        } = self;
        addresses.into_iter().map(move |member| {
            let at = coordinate(&member);
            (
                at.row.wrapping_sub(row_origin) as usize,
                at.col.wrapping_sub(column_origin) as usize,
                member,
            )
        })
    }

    /// Identify the worksheet shared by all admitted physical coordinates.
    pub(in crate::eval) fn sheet(&self) -> usize {
        self.sheet
    }
}

impl StoredPositions {
    /// Yield only the logical offsets for callers that need no source content.
    pub(in crate::eval) fn positions(self) -> impl ExactSizeIterator<Item = (usize, usize)> {
        self.members(|at| *at).map(|(row, column, _)| (row, column))
    }
}
