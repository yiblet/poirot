/// Cumulative preparation counters for one [`FormulaCompiler`](super::FormulaCompiler). Reusing prepared syntax
/// reduces parse/bind attempts; these counters do not count formula evaluations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilerUsage {
    /// Formula sources sent to the parser.
    pub parse_attempts: u64,
    /// Parsed sources sent to handler binding.
    pub bind_attempts: u64,
    /// Exact source-and-policy cache hits.
    pub reused_formulas: u64,
}
