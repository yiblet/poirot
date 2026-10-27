use super::prefix_key::PrefixKey;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Only bounded finite-number/boolean keys and coordinates use this hasher.
// Equality still compares the complete key; arbitrary strings never enter it.
/// Fast map restricted to bounded numeric/boolean lookup-prefix keys.
type PrefixMap<K, V> = HashMap<K, V, foldhash::fast::RandomState>;

/// Search progress along one lookup axis, including the first known match for each retained key.
/// The next position and exhaustion marker let later queries resume scanning without rereading the prefix.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(in crate::eval) struct SearchPrefix {
    pub(in crate::eval) first: PrefixMap<PrefixKey, usize>,
    // Original UTF-16 query identity preserves every ordinary comparison rule.
    // Arbitrary text uses the standard randomized hasher, never PrefixMap.
    pub(in crate::eval) text: HashMap<Vec<u16>, Option<usize>>,
    // Retained entries plus owned units, so admission recount is constant per axis.
    #[serde(skip)]
    pub(in crate::eval) text_storage: usize,
    pub(in crate::eval) next: usize,
    pub(in crate::eval) exhausted: bool,
}
