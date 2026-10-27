use poirot_functions::lookup::{Function, ReferenceFunction as ReferenceHelper};

/// Reference construction, selection, and shape handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ReferenceFunction {
    /// ADDRESS handler.
    Address,
    /// AREAS handler.
    Areas,
    /// HYPERLINK handler.
    Hyperlink,
    /// INDIRECT handler.
    Indirect,
    /// OFFSET handler.
    Offset,
    /// SHEET handler.
    Sheet,
    /// SINGLE implicit intersection handler.
    Single,

    /// ROWS handler.
    Rows,
    /// COLUMNS handler.
    Columns,
    /// ROW handler.
    Row,
    /// COLUMN handler.
    Column,
    /// INDEX handler.
    Index,
    /// Stored spelling of the postfix spill-range reference operator.
    AnchorArray,
}

impl ReferenceFunction {
    /// Wrap the family helper, or return `None` for evaluator-owned handlers.
    pub(in crate::eval) fn helper(self) -> Option<Function> {
        match self {
            Self::Indirect => Some(Function::Reference(ReferenceHelper::Indirect)),
            Self::Address
            | Self::Areas
            | Self::Hyperlink
            | Self::Sheet
            | Self::Rows
            | Self::Columns
            | Self::Row
            | Self::Column
            | Self::Index
            | Self::AnchorArray
            | Self::Single
            | Self::Offset => None,
        }
    }
}
