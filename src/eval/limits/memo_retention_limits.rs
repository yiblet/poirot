/// Logical bounds on owned Session memo retention, independent of calculation budgets.
/// Range readers borrow authored literals directly; those reads do not retain memo entries.
/// Each accepted entry costs 128 fixed units plus its owned text storage bytes.
/// These are deterministic retention units, not an allocator-exact heap measure.
/// Defaults are unlimited; `usize::MAX` disables the respective policy limit.
/// Intrinsic size overflow remains an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoRetentionLimits {
    /// Maximum retained memo entries.
    pub cells: usize,
    /// Maximum retained logical storage bytes.
    pub bytes: usize,
}

impl Default for MemoRetentionLimits {
    fn default() -> Self {
        Self {
            cells: usize::MAX,
            bytes: usize::MAX,
        }
    }
}

impl MemoRetentionLimits {
    /// Set maximum cached entries across the retained state's lifetime; zero admits none.
    /// Admissions are checked before publication; tightening below usage explicitly refuses.
    pub fn max_cached_cells(mut self, maximum: usize) -> Self {
        self.cells = maximum;
        self
    }

    /// Remove the cached entries quota without clearing retained state.
    pub fn without_cached_cells_limit(mut self) -> Self {
        self.cells = usize::MAX;
        self
    }

    /// Inspect the cached entries quota; `None` means no configured maximum.
    pub fn cached_cells_limit(&self) -> Option<usize> {
        (self.cells != usize::MAX).then_some(self.cells)
    }

    /// Set maximum logical retention bytes (128 fixed units per entry plus owned text) across the retained state's lifetime; zero admits none.
    /// Admissions are checked before publication; tightening below usage explicitly refuses.
    pub fn max_logical_memo_bytes(mut self, maximum: usize) -> Self {
        self.bytes = maximum;
        self
    }

    /// Remove the logical retention bytes (128 fixed units per entry plus owned text) quota without clearing retained state.
    pub fn without_logical_memo_bytes_limit(mut self) -> Self {
        self.bytes = usize::MAX;
        self
    }

    /// Inspect the logical retention bytes (128 fixed units per entry plus owned text) quota; `None` means no configured maximum.
    pub fn logical_memo_bytes_limit(&self) -> Option<usize> {
        (self.bytes != usize::MAX).then_some(self.bytes)
    }
}
