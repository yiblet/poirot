//! Retained evidence that lets later runs reuse earlier results: per-result consumed
//! reads and execution properties, the persistent result cache with its invalidation
//! indexes, spill ownership of retained results, and reusable exact-search prefixes.
//!
//! Depends on: config (context identity and consumed context fields), functions
//! (dependency policies and extension cacheability), coordinates (targets and
//! coordinate tables), limits (failures and retention limits) and ports (consumed-read
//! events).

/// Persistent results with reverse edges and range indexes for transitive invalidation.
mod cache;
/// One retained result and the dependencies needed to validate its reuse.
mod cached;
/// Reads and execution properties that determine when one cached result expires.
mod dependencies;
/// Sheet index and cell address identifying a dependency graph node.
mod key;
/// Reusable exact-search prefixes retained across compatible lookup calls.
mod lookup_prefixes;
/// Dirty causes in consumed order.
mod ordered_causes;
/// Physical lookup interval identifying one reusable search prefix.
mod prefix_axis;
/// Inclusive endpoints of a search prefix within its partition.
mod prefix_interval;
/// Finite numeric or boolean exact-match key of a search prefix.
mod prefix_key;
/// Sheet, orientation and fixed line of a search prefix.
mod prefix_partition;
/// Per-sheet lookup of consumers whose read rectangles contain a changed cell.
mod range_index;
/// Which dependency rectangles a range index resolves.
mod range_purpose;
/// Interval tree of read rectangles on one sheet.
mod range_tree;
/// Consumed dependencies and completed cells accumulated during one evaluation.
mod reads;
/// Search progress along one lookup axis.
mod search_prefix;
/// Spill ownership recorded with a retained result.
mod spill_role;
/// Unit tests for cached results, range indexes, recorded reads and lookup prefix snapshots.
#[cfg(test)]
mod tests;
/// Immediate invalidation evidence of a retained result.
mod validity;
/// Ordered actual value reads of one result.
mod value_read_order;

pub(in crate::eval) use cache::Cache;
pub(in crate::eval) use cached::Cached;
pub(in crate::eval) use dependencies::Dependencies;
pub(in crate::eval) use key::Key;
pub(in crate::eval) use lookup_prefixes::{LookupPrefixes, MAX_ENTRIES};
pub(in crate::eval) use prefix_axis::PrefixAxis;
pub(in crate::eval) use prefix_key::PrefixKey;
pub(in crate::eval) use reads::Reads;
pub(in crate::eval) use search_prefix::SearchPrefix;
pub(in crate::eval) use spill_role::SpillRole;
pub(in crate::eval) use validity::Validity;
