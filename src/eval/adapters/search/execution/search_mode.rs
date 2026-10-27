/// Exact matching or approximate matching against an ordered vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    /// Select an exact match, applying source wildcard rules where supported.
    Exact,
    /// Approximate matching under ascending-order rules.
    Ascending,
    /// Approximate matching under descending-order rules.
    Descending,
}

impl SearchMode {
    pub(in crate::eval::adapters::search) fn from_number(value: f64) -> Self {
        if value == 0. {
            Self::Exact
        } else if value > 0. {
            Self::Ascending
        } else {
            Self::Descending
        }
    }
}
