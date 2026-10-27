use crate::model::CellAddress;

/// Sheet index and cell address identifying a graph node.
pub(in crate::eval) type Key = (usize, CellAddress);
