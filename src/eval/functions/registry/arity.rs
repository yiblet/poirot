/// Inclusive minimum and maximum argument counts accepted by a function contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Arity {
    /// Smallest accepted argument count.
    pub minimum: usize,
    /// Largest accepted argument count.
    pub maximum: usize,
}

impl Arity {
    /// Check the inclusive argument-count bounds.
    pub fn accepts(self, count: usize) -> bool {
        (self.minimum..=self.maximum).contains(&count)
    }
}
