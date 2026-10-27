use crate::model::CellValue;

/// Coordinate-keyed memo moved intact from a completed or aborted evaluation.
///
/// This uses a randomized noncryptographic hasher for validated numeric keys.
/// Keys, values and map operations match `HashMap`; explicitly typed consumers
/// should use this alias rather than a `HashMap` with the default hasher.
pub type CellMemo = std::collections::HashMap<
    (usize, crate::model::CellAddress),
    CellValue,
    foldhash::fast::RandomState,
>;
