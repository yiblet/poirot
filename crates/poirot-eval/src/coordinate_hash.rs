//! Hash tables for run-local, validated worksheet coordinates.
//!
//! Keys retain the full sheet index and address, and ordinary key equality still
//! resolves hash collisions. The row/column domain is bounded by worksheet limits;
//! sheet indices come from the workbook. These tables never hash formula text or
//! arbitrary user strings. FoldHash randomizes each table to avoid predictable
//! collision sets, but is not cryptographic and does not promise resistance to an
//! adaptive attacker recovering its seed. Run work and memo retention limits
//! remain the controls on evaluation resources.

use poirot_model::CellAddress;
use std::collections::{HashMap, HashSet};

/// Coordinate-keyed map using randomized hashing for validated numeric keys.
pub(crate) type CellMap<V> = HashMap<(usize, CellAddress), V, foldhash::fast::RandomState>;
/// Coordinate set using randomized hashing for validated numeric keys.
pub(crate) type CellSet = HashSet<(usize, CellAddress), foldhash::fast::RandomState>;
