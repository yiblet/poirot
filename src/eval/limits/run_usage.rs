/// Execution counters for one run, distinguishing formula execution, prepared lookups,
/// session memo hits, and validated cross-run cache hits.
/// Counters saturate at `u64::MAX`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunUsage {
    /// Runtime parsing of computed reference text; excluded from pure preparation counters.
    pub dynamic_parse_attempts: u64,
    /// Formula cell bodies executed.
    pub formula_executions: u64,
    /// Defined-name formula bodies executed.
    pub named_formula_executions: u64,
    /// Prepared formula lookups performed.
    pub compiled_lookups: u64,
    /// Reads satisfied by this run's memo.
    pub session_memo_hits: u64,
    /// Exact lookup selections satisfied without scanning candidate cells.
    pub lookup_prefix_hits: u64,
    /// Candidate cells scanned while extending eligible exact lookup indexes.
    pub lookup_prefix_scans: u64,
    /// Reads satisfied by a validated cross-run cache.
    pub external_cache_hits: u64,
}
