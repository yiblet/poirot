/// Position in a compiled body's instruction vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InstructionIndex(pub u32);

impl InstructionIndex {
    /// Convert to a slice index for bounds-checked instruction access.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}
