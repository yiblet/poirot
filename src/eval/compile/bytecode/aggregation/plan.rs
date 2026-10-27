use super::candidate::Candidate;

/// Identifiers lowered from a grouping function's aggregation argument.
/// The scalar or one-axis vector shape lets the host select aggregation handlers
/// without treating those identifiers as ordinary worksheet values.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    /// Number of vector rows.
    pub rows: usize,
    /// Number of vector columns.
    pub columns: usize,
    /// Row-major identifiers; worksheet values cannot inhabit this channel.
    pub members: Vec<Candidate>,
}

impl Plan {
    /// Check bounded vector geometry and identifier storage after deserialization.
    pub fn valid(&self) -> bool {
        self.rows != 0
            && self.columns != 0
            && (self.rows == 1 || self.columns == 1)
            && self.rows.checked_mul(self.columns) == Some(self.members.len())
            && self.members.len() <= 100_000
            && self.members.iter().all(|n| {
                !n.name.is_empty()
                    && n.name.len() <= 32767
                    && n.sheet
                        .as_ref()
                        .is_none_or(|s| !s.is_empty() && s.len() <= 32767)
            })
    }
}
