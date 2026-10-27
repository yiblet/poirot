use super::aggregation;
use super::instruction::Instruction;
use super::instruction_index::InstructionIndex;
use super::label::Label;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// Shared instructions and literal operand bytes. Reference instructions name
/// slots; their concrete coordinates live in each formula's [`ReferenceBindings`](super::ReferenceBindings).
pub struct Template {
    /// Dense mapping from label IDs to instruction positions in this body.
    pub labels: Vec<InstructionIndex>,
    /// Contextual aggregation projections keyed by expression entry.
    pub aggregations: Vec<aggregation::Entry>,
    /// Executable instructions in instruction-pointer order.
    pub code: Vec<Instruction>,
    /// Packed literal and name bytes addressed by instructions.
    #[serde(with = "serde_bytes")]
    pub operands: Vec<u8>,
    /// Number of per-formula reference slots required by this template.
    pub reference_slots: u32,
}

impl Template {
    /// Resolve a body-local label without changing instructions or retaining pointers into vectors.
    pub fn resolve_label(&self, label: Label) -> Option<InstructionIndex> {
        self.labels.get(label.0 as usize).copied()
    }
}
