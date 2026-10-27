/// Work counters from a completed or failed calculation.
///
/// These measure engine operations, not elapsed time or total heap allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluationUsage {
    /// Cell-local random stream accesses observed during execution.
    pub random_accesses: Vec<super::random_access::RandomAccess>,
    /// Total charged work units.
    pub work_units: u64,
    /// Admitted expression nodes.
    pub expressions: u64,
    /// Cell read operations.
    pub cell_reads: u64,
    /// Reads served from an execution cache.
    pub cache_hits: u64,
    /// Sparse range entries inspected.
    pub sparse_entries: u64,
    /// Name or sheet namespace entries inspected.
    pub namespace_entries: u64,
    /// Logical values charged for materialization.
    pub materialized_values: u64,
    /// Charged comparisons.
    pub comparisons: u64,
    /// Charged scalar operators.
    pub scalar_operators: u64,
    /// Charged builtin helper operations.
    pub helper_operations: u64,
    /// Charged wildcard matching states.
    pub wildcard_states: u64,
    /// Charged text processing bytes.
    pub text_bytes: u64,
    /// Charged calendar steps.
    pub calendar_steps: u64,
    /// Charged reference pairs.
    pub reference_pairs: u64,
    /// Greatest observed nesting depth.
    pub maximum_depth: usize,
    /// Cells whose execution completed.
    pub completed_cells: u64,
}
