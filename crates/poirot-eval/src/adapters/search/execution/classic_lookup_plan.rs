//! Validates lookup table shapes and maps vector indices to table coordinates.
//! Geometry checks do not evaluate cells; the search host performs reads and charges work.
use super::*;

/// Validated lookup operand geometry with lazy workbook reads preserved.
pub(super) enum LookupTable {
    /// Single physical reference area.
    Reference(Area),
    /// Logical area with signed, possibly off-grid coordinates.
    Signed(signed_reference::SignedArea),
    /// Singleton cells across a sheet span accepted by MATCH.
    SheetCells(Vec<Area>),
    /// Materialized rectangular values.
    Array(ArrayView),
}
impl LookupTable {
    fn reference(a: Area) -> Result<Self, CellError> {
        if a.r0 > a.r1
            || a.c0 > a.c1
            || (CellAddress {
                row: a.r1,
                col: a.c1,
            })
            .validate()
            .is_err()
        {
            return Err(Ref);
        }
        Ok(Self::Reference(a))
    }
    /// Return logical row and column counts without reading table cells.
    pub(super) fn shape(&self) -> (usize, usize) {
        match self {
            Self::SheetCells(a) => (a.len(), 1),
            Self::Signed(a) => (a.height() as usize, a.width() as usize),
            Self::Reference(a) => ((a.r1 - a.r0 + 1) as usize, (a.c1 - a.c0 + 1) as usize),
            Self::Array(a) => (a.len(), a.first().map_or(0, Vec::len)),
        }
    }
    /// Validate source-specific table channels and shape before lookup traversal.
    pub(super) fn admit(value: Value, is_match: bool) -> Result<Self, CellError> {
        match value {
            Value::SheetSpan(a) if !a.is_empty() => {
                // SAFETY: The match guard requires a nonempty vector or exactly one area before reading element zero.
                if is_match && a[0].kind == AreaKind::Cell {
                    if a.iter().any(|area| {
                        Self::reference(*area).is_err() || area.r0 != area.r1 || area.c0 != area.c1
                    }) {
                        return Err(Ref);
                    }
                    Ok(Self::SheetCells(a))
                } else {
                    // SAFETY: The match guard requires a nonempty vector or exactly one area before reading element zero.
                    Self::reference(a[0])
                }
            }
            // SAFETY: The match guard requires a nonempty vector or exactly one area before reading element zero.
            Value::Refs(a) if a.len() == 1 => Self::reference(a[0]),
            // SAFETY: The match guard requires a nonempty vector or exactly one area before reading element zero.
            Value::SignedRefs(a) if a.len() == 1 => Ok(Self::Signed(a[0])),
            Value::Array(a) => {
                let width = a.first().map_or(0, Vec::len);
                if a.iter().any(|r| r.len() != width) {
                    return Err(VALUE_ERROR);
                }
                Ok(Self::Array(a))
            }
            Value::Scalar(_) if is_match => Err(NA),
            _ => Err(VALUE_ERROR),
        }
    }
    /// Select a nonempty row or column vector at a fixed cross-axis offset.
    pub(super) fn vector(&self, axis: SearchAxis, fixed: usize) -> Result<LookupVector, CellError> {
        let (height, width) = self.shape();
        let (len, bound) = match axis {
            SearchAxis::Vertical => (height, width),
            SearchAxis::Horizontal => (width, height),
        };
        if len == 0 || fixed >= bound {
            return Err(Ref);
        }
        Ok(LookupVector { axis, fixed, len })
    }
}
/// Logical row or column selection within a validated table.
#[derive(Clone, Copy)]
pub(super) struct LookupVector {
    /// Axis along which candidate indices advance.
    pub(super) axis: SearchAxis,
    /// Zero-based position along the other axis.
    pub(super) fixed: usize,
    /// Number of candidate positions.
    pub(super) len: usize,
}
impl LookupVector {
    /// Map a checked candidate index to zero-based table row and column offsets.
    pub(super) fn coordinates(self, index: usize) -> Result<(usize, usize), CellError> {
        if index >= self.len {
            return Err(Ref);
        }
        Ok(match self.axis {
            SearchAxis::Vertical => (index, self.fixed),
            SearchAxis::Horizontal => (self.fixed, index),
        })
    }
}
