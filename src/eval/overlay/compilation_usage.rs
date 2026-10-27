/// Cumulative compilation work, independent of formula execution counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilationUsage {
    /// Number of formula parsing attempts.
    pub parse_attempts: u64,
    /// Number of syntax-to-handler binding attempts.
    pub bind_attempts: u64,
    /// Number of exact compiled formula cache hits.
    pub reused_formulas: u64,
    /// Number of bytecode template reuse hits.
    pub template_hits: u64,
    /// Number of distinct retained bytecode programs.
    pub unique_programs: usize,
}
