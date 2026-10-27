//! Evaluator-owned classic lookup reader, sparse traversal, and prefix reuse.
//!
//! Depends on: host (CallHost), session (reads and coercion), spill (SpillDemand and
//! SpillQuery), config (policies), limits (budgets), values (value channels and
//! references), ports (dependency reads), inputs (effective workbook cells) and reuse
//! (retained lookup prefixes).

mod comparison;
#[cfg(test)]
mod historical_selection;
mod plan;
mod prefix;
/// Direction in which a logical lookup vector traverses its table.
mod search_axis;
/// Exact matching or approximate matching against an ordered vector.
mod search_mode;
/// Zero-based position selected within the logical lookup vector, not an absolute worksheet row/column.
mod search_selection;
mod selection;
/// Session entry points for classic lookup and match execution.
mod session_search;
mod sparse;
/// Unit tests for session-backed lookup execution.
#[cfg(test)]
mod tests;

pub(in crate::eval) use plan::LookupTable;
/// Bounded exact-lookup prefixes, retained across successful overlay evaluations.
pub use search_axis::SearchAxis;
pub use search_mode::SearchMode;
pub use search_selection::SearchSelection;
pub(in crate::eval) use session_search::SessionSearch;
