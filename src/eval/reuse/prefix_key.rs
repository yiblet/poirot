use crate::model::CellValue;
use crate::model::CellValue::{Blank, Boolean, Number};
use serde::{Deserialize, Serialize};

/// Finite numeric or boolean exact-match key with normalized signed zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(in crate::eval) enum PrefixKey {
    Number(u64),
    Boolean(bool),
}

impl PrefixKey {
    pub(in crate::eval) fn candidate(value: &CellValue) -> Option<Self> {
        match value {
            Number(n) if n.is_finite() => {
                Some(Self::Number(if *n == 0. { 0 } else { n.to_bits() }))
            }
            Boolean(b) => Some(Self::Boolean(*b)),
            _ => None,
        }
    }

    pub(in crate::eval) fn search(value: &CellValue) -> Option<Self> {
        if *value == Blank {
            Some(Self::Number(0))
        } else {
            Self::candidate(value)
        }
    }
}
