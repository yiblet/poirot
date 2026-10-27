//! Hash set of run-local, validated worksheet coordinates; the hashing rationale is
//! documented beside `CellMap`.

use crate::model::CellAddress;
use std::collections::HashSet;

/// Coordinate set using randomized hashing for validated numeric keys.
pub(in crate::eval) type CellSet = HashSet<(usize, CellAddress), foldhash::fast::RandomState>;
