use super::plan::Plan;

/// Associates an expression's bytecode entry with its aggregation-role interpretation.
/// Grouping calls can retrieve this prepared projection without rebuilding syntax.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Entry instruction pointer, always an Enter instruction.
    pub start: usize,
    /// Typed role projection without retained syntax or executable reconstruction.
    pub plan: Plan,
}
