//! One authored mutation of a cell.

use crate::model::Cell;

/// One authored mutation; absent entries inherit baseline content and ownership.
#[derive(Clone)]
pub(in crate::eval) enum CellEdit {
    /// Explicit content replacement, including an authored blank.
    Replace(Box<Cell>),
    /// Changed style while baseline content and saved ownership remain authoritative.
    StyleOnly(Box<Cell>),
    /// Hidden baseline source, distinct from an authored blank.
    Delete,
}

impl CellEdit {
    /// Attach the binary baseline-preserving style property to stored content.
    pub(super) fn with_cell(cell: Box<Cell>, style_only: bool) -> Self {
        if style_only {
            Self::StyleOnly(cell)
        } else {
            Self::Replace(cell)
        }
    }

    /// Borrow content when this edit stores a cell.
    pub(in crate::eval) fn cell(&self) -> Option<&Cell> {
        match self {
            Self::Replace(cell) | Self::StyleOnly(cell) => Some(cell),
            Self::Delete => None,
        }
    }

    /// Whether this edit preserves baseline content and ownership.
    pub(in crate::eval) fn is_style_only(&self) -> bool {
        matches!(self, Self::StyleOnly(_))
    }
}
