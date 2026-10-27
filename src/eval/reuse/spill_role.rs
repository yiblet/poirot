use crate::model::CellAddress;

/// Spill roles used by private admissions and settled retained publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::eval) enum SpillRole {
    /// A scalar result without a generated footprint.
    #[default]
    None,
    /// Producer whose complete rectangular footprint ends at this address.
    Anchor { last: CellAddress },
    /// Generated result owned by another source cell on the same sheet.
    Child { anchor: CellAddress },
}

impl SpillRole {
    /// Resolve ownership relative to the address holding this record.
    pub(in crate::eval) fn owner(self, address: CellAddress) -> Option<CellAddress> {
        match self {
            Self::None => None,
            Self::Anchor { .. } => Some(address),
            Self::Child { anchor } => Some(anchor),
        }
    }

    /// Return the admitted extent only for a producer.
    pub(in crate::eval) fn last(self) -> Option<CellAddress> {
        match self {
            Self::Anchor { last } => Some(last),
            _ => None,
        }
    }
}
