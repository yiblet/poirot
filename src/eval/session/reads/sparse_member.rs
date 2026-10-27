//! A sparse range member and its already inspected source.

use crate::model::CellAddress;

/// Carries a sparse range member's address and already inspected source into value projection.
/// Borrowing authored content avoids a second lookup; generated spill members still
/// use ordinary cell resolution to obtain their values.
#[derive(Clone, Copy)]
pub(in crate::eval) struct SparseMember<'a> {
    pub(in crate::eval) address: CellAddress,
    /// None identifies generated-only spill members, which require ordinary source resolution.
    pub(in crate::eval) cell: Option<&'a crate::model::Cell>,
}
